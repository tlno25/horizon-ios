import CryptoKit
import Foundation
import SwiftUI
import UIKit

/// The publisher supplies a JSON manifest URL once when making the app.
/// Players never have to navigate Files or paste an archive URL.
private struct GameFilesManifest: Decodable {
    let minecraft: String
    let url: URL
    let sha256: String
    let size: Int64

    func validate(expectedVersion: String) throws {
        guard minecraft == expectedVersion else { throw DownloadProblem("ゲームファイルのバージョンが違います。") }
        guard size > 0 && size <= 2 * 1024 * 1024 * 1024 else {
            throw DownloadProblem("ダウンロードサイズが不正です。")
        }
        guard url.scheme?.lowercased() == "https", url.host != nil,
              url.user == nil, url.password == nil, url.fragment == nil else {
            throw DownloadProblem("ゲームファイルはHTTPSからのみ取得できます。")
        }
        guard sha256.count == 64 && sha256.utf8.allSatisfy({
            ($0 >= 48 && $0 <= 57) || ($0 >= 65 && $0 <= 70) || ($0 >= 97 && $0 <= 102)
        }) else { throw DownloadProblem("ダウンロードのSHA-256が不正です。") }
    }
}

private struct DownloadProblem: LocalizedError {
    let errorDescription: String?
    init(_ value: String) { errorDescription = value }
}

/// A single active download; the Rust importer owns ZIP extraction and all
/// resource path/version checks. No game data is shipped inside this framework.
@MainActor
final class NativeGameDownload: NSObject, ObservableObject {
    @Published private(set) var message = ""
    @Published private(set) var busy = false
    @Published private(set) var installing = false
    @Published private(set) var downloadedBytes: Int64 = 0
    @Published private(set) var totalBytes: Int64 = 0
    @Published private(set) var installed = false

    private var session: URLSession?
    private var manifest: GameFilesManifest?
    private var generation = UUID()
    private nonisolated static let limit: Int64 = 2 * 1024 * 1024 * 1024

    var progress: Double? {
        totalBytes > 0 ? min(1, Double(downloadedBytes) / Double(totalBytes)) : nil
    }

    func begin(version: String) async {
        guard !busy else { return }
        guard let source = Bundle.main.object(forInfoDictionaryKey: "HorizonResourcesManifestURL") as? String,
              let url = URL(string: source), url.scheme == "https", url.host != nil else {
            message = "ゲームファイルの配信元が設定されていません。配布元の設定が必要です。"
            return
        }
        let id = UUID()
        generation = id
        busy = true
        installing = false
        downloadedBytes = 0
        totalBytes = 0
        message = "配信情報を確認中…"
        let session = URLSession(configuration: .default, delegate: self, delegateQueue: nil)
        self.session = session
        do {
            let (data, response) = try await session.data(for: URLRequest(url: url))
            guard generation == id, self.session === session else { return }
            guard (response as? HTTPURLResponse)?.statusCode == 200, data.count <= 32 * 1024 else {
                throw DownloadProblem("配信情報を取得できませんでした。")
            }
            let manifest = try JSONDecoder().decode(GameFilesManifest.self, from: data)
            try manifest.validate(expectedVersion: version)
            self.manifest = manifest
            totalBytes = manifest.size
            message = "ゲームファイルをダウンロード中…"
            session.downloadTask(with: URLRequest(url: manifest.url)).resume()
        } catch {
            fail(error.localizedDescription, from: session)
        }
    }

    func cancel() {
        guard busy && !installing else { return }
        generation = UUID()
        let current = session
        session = nil
        current?.invalidateAndCancel()
        busy = false
        message = "ダウンロードをキャンセルしました。"
    }

    private func fail(_ reason: String, from current: URLSession) {
        guard session === current else { return }
        generation = UUID()
        session = nil
        current.invalidateAndCancel()
        busy = false
        installing = false
        message = reason
    }

    fileprivate func finishDownload(_ archive: URL, from current: URLSession) {
        guard session === current, let manifest else {
            try? FileManager.default.removeItem(at: archive)
            return
        }
        installing = true
        message = "ダウンロードを検証中…"
        let id = generation
        DispatchQueue.global(qos: .userInitiated).async {
            let result: Result<Void, Error>
            do {
                let file = try FileHandle(forReadingFrom: archive)
                defer { try? file.close() }
                var digest = SHA256()
                var length: Int64 = 0
                while let data = try file.read(upToCount: 64 * 1024), !data.isEmpty {
                    digest.update(data: data)
                    length += Int64(data.count)
                    if length > Self.limit { throw DownloadProblem("ZIPが2GiBを超えています。") }
                }
                let actual = digest.finalize().map { String(format: "%02x", $0) }.joined()
                guard length == manifest.size && actual == manifest.sha256.lowercased() else {
                    throw DownloadProblem("ダウンロードしたデータが一致しません。再試行してください。")
                }
                result = .success(())
            } catch {
                result = .failure(error)
            }
            Task { @MainActor in
                guard self.session === current, self.generation == id else {
                    try? FileManager.default.removeItem(at: archive)
                    return
                }
                switch result {
                case .failure(let error):
                    try? FileManager.default.removeItem(at: archive)
                    self.fail(error.localizedDescription, from: current)
                case .success:
                    self.message = "ファイルをインストール中…"
                    let answer = await Bridge.send("launcher-import \(archive.lastPathComponent)")
                    try? FileManager.default.removeItem(at: archive)
                    guard self.session === current, self.generation == id else { return }
                    if let issue = answer.error {
                        self.fail(issue, from: current)
                    } else {
                        self.session = nil
                        current.finishTasksAndInvalidate()
                        self.busy = false
                        self.installing = false
                        self.message = "インストールが完了しました。ゲームを起動します…"
                        self.installed = true
                    }
                }
            }
        }
    }
}

extension NativeGameDownload: URLSessionDownloadDelegate {
    nonisolated func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask,
                                didWriteData _: Int64, totalBytesWritten: Int64,
                                totalBytesExpectedToWrite: Int64) {
        let overflow = totalBytesWritten > Self.limit || totalBytesExpectedToWrite > Self.limit
        if overflow { downloadTask.cancel() }
        Task { @MainActor in
            guard self.session === session else { return }
            if overflow {
                self.fail("ダウンロードが上限の2GiBを超えました。", from: session)
            } else {
                self.downloadedBytes = totalBytesWritten
                if let expected = self.manifest?.size {
                    self.totalBytes = expected
                }
            }
        }
    }

    nonisolated func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask,
                                didFinishDownloadingTo location: URL) {
        let code = (downloadTask.response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200..<300).contains(code), downloadTask.countOfBytesReceived <= Self.limit else {
            Task { @MainActor in self.fail("ダウンロードに失敗しました (HTTP \(code))。", from: session) }
            return
        }
        // Move inside the delegate: iOS deletes the original temporary file at return.
        guard let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first else {
            Task { @MainActor in self.fail("アプリの保存先を取得できません。", from: session) }
            return
        }
        let archive = docs.appendingPathComponent("horizon-download-\(UUID().uuidString).zip")
        do {
            try FileManager.default.moveItem(at: location, to: archive)
            Task { @MainActor in self.finishDownload(archive, from: session) }
        } catch {
            Task { @MainActor in self.fail("ZIPを保存できません: \(error.localizedDescription)", from: session) }
        }
    }

    nonisolated func urlSession(_ session: URLSession, task: URLSessionTask,
                                didCompleteWithError error: Error?) {
        guard let error else { return }
        Task { @MainActor in self.fail(error.localizedDescription, from: session) }
    }

    nonisolated func urlSession(_ session: URLSession, task: URLSessionTask,
                                willPerformHTTPRedirection response: HTTPURLResponse,
                                newRequest request: URLRequest,
                                completionHandler: @escaping (URLRequest?) -> Void) {
        guard let url = request.url, url.scheme?.lowercased() == "https", url.host != nil,
              url.user == nil, url.password == nil else {
            completionHandler(nil)
            return
        }
        completionHandler(request)
    }
}

struct GameLauncherView: View {
    let onPlay: () -> Void
    @StateObject private var download = NativeGameDownload()
    @State private var hasResources = false
    @State private var version = "26.3"
    @State private var notice = ""
    @State private var showSettings = false
    @State private var starting = false

    var body: some View {
        ZStack {
            Color(uiColor: .systemBackground).ignoresSafeArea()
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    HStack {
                        VStack(alignment: .leading, spacing: 3) {
                            Text("HORIZON").font(.system(size: 33, weight: .bold, design: .rounded))
                            Text("Minecraft: Java Edition \(version)")
                                .font(.subheadline).foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button { showSettings = true } label: {
                            Label("設定", systemImage: "gearshape")
                        }.buttonStyle(.bordered)
                    }
                    Divider()
                    if hasResources {
                        Label("ゲームファイルを準備できました", systemImage: "checkmark.circle.fill")
                            .foregroundStyle(.green)
                        Button { Task { await startGame() } } label: {
                            Label("プレイ", systemImage: "play.fill")
                                .frame(maxWidth: .infinity, minHeight: 40)
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(starting)
                    } else {
                        Text("ゲームファイルのセットアップ").font(.title3.bold())
                        Text("このアプリから必要なデータをダウンロードしてインストールします。フォルダ操作は不要です。")
                            .font(.subheadline).foregroundStyle(.secondary)
                        if let value = download.progress, download.busy && !download.installing {
                            ProgressView(value: value)
                            Text("\(Int(value * 100))% · \(download.downloadedBytes / 1_048_576) MiB")
                                .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                        } else if download.busy {
                            ProgressView()
                        }
                        if !download.busy {
                            Button { Task { await download.begin(version: version) } } label: {
                                Label("ゲームファイルをダウンロード", systemImage: "arrow.down.circle")
                                    .frame(maxWidth: .infinity, minHeight: 40)
                            }.buttonStyle(.borderedProminent)
                        } else if !download.installing {
                            Button("キャンセル", role: .cancel) { download.cancel() }
                                .buttonStyle(.bordered)
                        }
                    }
                    if !download.message.isEmpty {
                        Text(download.message).font(.footnote).foregroundStyle(.secondary)
                            .textSelection(.enabled)
                    }
                    if !notice.isEmpty {
                        Text(notice).font(.footnote).foregroundStyle(.red)
                            .textSelection(.enabled)
                    }
                    Divider()
                    Text("Minecraft: Java Editionの正規ライセンスが必要です。HorizonはMojang/Microsoftの公式製品ではありません。")
                        .font(.caption).foregroundStyle(.secondary)
                }
                .padding(24)
                .frame(maxWidth: 620, alignment: .leading)
                .frame(maxWidth: .infinity)
            }
        }
        .sheet(isPresented: $showSettings) { NativeSettingsView() }
        .task { await refresh() }
        .onChange(of: download.installed) { installed in
            if installed { Task { await startGame() } }
        }
    }

    private func refresh() async {
        let status = await Bridge.send("launcher-state")
        if let version = status["version"] as? String { self.version = version }
        hasResources = status["ready"] as? Bool ?? false
        if let issue = status.error { notice = issue }
    }

    private func startGame() async {
        guard !starting else { return }
        starting = true
        let reply = await Bridge.send("launcher-start")
        if let error = reply.error {
            notice = error
            starting = false
            return
        }
        onPlay()
    }
}

private struct NativeSettingsView: View {
    @Environment(\.dismiss) private var dismiss
    @State private var playerName = ""
    @State private var renderDistance = 8
    @State private var simulationDistance = 8
    @State private var maxFps = 60
    @State private var fov = 70
    @State private var volume = 1.0
    @State private var autoJump = false
    @State private var vsync = true
    @State private var message = ""

    var body: some View {
        NavigationStack {
            Form {
                Section("プレイヤー") {
                    TextField("オフラインプレイヤー名", text: $playerName)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    Text("半角英数字とアンダースコアで3〜16文字。Microsoftアカウントでのログインとは別です。")
                        .font(.footnote).foregroundStyle(.secondary)
                }
                Section("グラフィックス") {
                    Stepper("描画距離: \(renderDistance)", value: $renderDistance, in: 2...24)
                    Stepper("シミュレーション距離: \(simulationDistance)", value: $simulationDistance, in: 5...16)
                    Stepper("最大FPS: \(maxFps)", value: $maxFps, in: 30...120, step: 10)
                    Stepper("視野角: \(fov)°", value: $fov, in: 30...110)
                    Toggle("垂直同期", isOn: $vsync)
                }
                Section("操作・サウンド") {
                    Toggle("自動ジャンプ", isOn: $autoJump)
                    HStack {
                        Text("音量")
                        Slider(value: $volume, in: 0...1, step: 0.05)
                    }
                }
                if !message.isEmpty { Text(message).font(.footnote) }
                Text("設定はゲーム本体と共有され、次回の起動時に反映されます。")
                    .font(.caption).foregroundStyle(.secondary)
            }
            .navigationTitle("ゲーム設定")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("閉じる") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) { Button("保存") { Task { await save() } } }
            }
            .task { await load() }
        }
    }

    private func load() async {
        let profile = await Bridge.send("launcher-player-get")
        if let name = profile["name"] as? String { playerName = name }
        let options = await Bridge.send("launcher-settings")
        if let error = options.error { message = error; return }
        renderDistance = options["renderDistance"] as? Int ?? renderDistance
        simulationDistance = options["simulationDistance"] as? Int ?? simulationDistance
        maxFps = options["maxFps"] as? Int ?? maxFps
        fov = options["fov"] as? Int ?? fov
        volume = options["soundCategory_master"] as? Double ?? volume
        autoJump = options["autoJump"] as? Bool ?? autoJump
        vsync = options["enableVsync"] as? Bool ?? vsync
    }

    private func save() async {
        guard (3...16).contains(playerName.utf8.count),
              playerName.utf8.allSatisfy({
                  ($0 >= 48 && $0 <= 57) || ($0 >= 65 && $0 <= 90) ||
                  ($0 >= 97 && $0 <= 122) || $0 == 95
              }) else {
            message = "プレイヤー名は3〜16文字の半角英数字・アンダースコアで入力してください。"
            return
        }
        let playerReply = await Bridge.send("launcher-player \(playerName)")
        if let error = playerReply.error { message = error; return }
        let values: [(String, String)] = [
            ("renderDistance", "\(renderDistance)"),
            ("simulationDistance", "\(simulationDistance)"),
            ("maxFps", "\(maxFps)"),
            ("fov", "\(fov)"),
            ("soundCategory_master", String(format: "%.2f", volume)),
            ("autoJump", "\(autoJump)"),
            ("enableVsync", "\(vsync)"),
        ]
        for (key, value) in values {
            guard let data = try? JSONSerialization.data(withJSONObject: ["key":key,"value":value]),
                  let json = String(data: data, encoding: .utf8) else {
                message = "設定の変換に失敗しました。"; return
            }
            let reply = await Bridge.send("launcher-option \(json)")
            if let error = reply.error { message = error; return }
        }
        message = "保存しました。"
    }
}
