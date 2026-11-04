import CryptoKit
import Foundation
import SwiftUI
import UIKit

/// Downloads a privately prepared Horizon resource ZIP. The actual manifest,
/// path and expanded-size checks remain in horizon_ios_import_resources (Rust).
/// This intentionally does not download copyrighted vanilla game files from an
/// unverified public mirror, nor attempt to run Java exporters on the phone.
@MainActor
final class PreparedResourceDownload: NSObject, ObservableObject {
    @Published private(set) var status = ""
    @Published private(set) var received: Int64 = 0
    @Published private(set) var total: Int64 = 0
    @Published private(set) var isBusy = false
    @Published private(set) var isInstalling = false

    private nonisolated static let maximumBytes: Int64 = 2 * 1024 * 1024 * 1024
    private var session: URLSession?
    private var expectedHash = ""
    private var attempt = UUID()

    var progress: Double? {
        total > 0 ? min(1, Double(received) / Double(total)) : nil
    }

    func start(link: String, checksum: String) {
        guard !isBusy else { return }
        let link = link.trimmingCharacters(in: .whitespacesAndNewlines)
        let hash = checksum.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard let url = URL(string: link), url.scheme?.lowercased() == "https",
              let host = url.host, !host.isEmpty, url.user == nil, url.password == nil,
              url.fragment == nil else {
            status = "HTTPSの直接ダウンロードURLを指定してください。"
            return
        }
        guard hash.utf8.count == 64,
              hash.utf8.allSatisfy({ ($0 >= 48 && $0 <= 57) || ($0 >= 97 && $0 <= 102) }) else {
            status = "64桁のSHA-256を入力してください。"
            return
        }
        expectedHash = hash
        attempt = UUID()
        received = 0
        total = 0
        status = "ダウンロード中…"
        isBusy = true
        isInstalling = false
        let session = URLSession(configuration: .default, delegate: self, delegateQueue: nil)
        self.session = session
        var request = URLRequest(url: url)
        request.timeoutInterval = 60
        session.downloadTask(with: request).resume()
    }

    func cancel() {
        guard !isInstalling else { return }
        attempt = UUID()
        let current = session
        session = nil
        current?.invalidateAndCancel()
        isBusy = false
        status = "キャンセルしました。"
    }

    private func failed(_ message: String, session current: URLSession) {
        guard session === current else { return }
        session = nil
        current.invalidateAndCancel()
        isInstalling = false
        isBusy = false
        status = message
    }

    private func accept(_ file: URL, from current: URLSession) {
        guard session === current else {
            try? FileManager.default.removeItem(at: file)
            return
        }
        isInstalling = true
        status = "SHA-256を検証中…"
        let hash = expectedHash
        let generation = attempt
        // Hash and ZIP extraction may take minutes. Do neither on UIKit.
        DispatchQueue.global(qos: .userInitiated).async {
            let result: String
            do {
                let handle = try FileHandle(forReadingFrom: file)
                defer { try? handle.close() }
                var digest = SHA256()
                while let chunk = try handle.read(upToCount: 64 * 1024), !chunk.isEmpty {
                    digest.update(data: chunk)
                }
                let calculated = digest.finalize().map { String(format: "%02x", $0) }.joined()
                guard calculated == hash else {
                    throw LauncherDownloadError("SHA-256が一致しません。元のファイルを確認してください。")
                }
                let path = Array(file.path.utf8)
                let code = path.withUnsafeBufferPointer { buffer in
                    horizon_ios_import_resources(buffer.baseAddress, buffer.count)
                }
                result = code == 0 ? "リソースのインストールが完了しました。" : "ZIPのインストールに失敗しました。ファイル構造を確認してください。"
            } catch {
                result = error.localizedDescription
            }
            try? FileManager.default.removeItem(at: file)
            DispatchQueue.main.async {
                guard self.attempt == generation, self.session === current else { return }
                self.session = nil
                current.finishTasksAndInvalidate()
                self.isBusy = false
                self.isInstalling = false
                self.status = result
            }
        }
    }
}

private struct LauncherDownloadError: LocalizedError {
    let errorDescription: String?
    init(_ message: String) { errorDescription = message }
}

extension PreparedResourceDownload: URLSessionDownloadDelegate {
    nonisolated func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask,
                                didWriteData _: Int64, totalBytesWritten: Int64,
                                totalBytesExpectedToWrite: Int64) {
        let tooLarge = totalBytesWritten > Self.maximumBytes ||
            totalBytesExpectedToWrite > Self.maximumBytes
        if tooLarge { downloadTask.cancel() }
        Task { @MainActor in
            guard self.session === session else { return }
            if tooLarge {
                self.failed("ZIPが2GiBの上限を超えています。", session: session)
            } else {
                self.received = totalBytesWritten
                self.total = max(0, totalBytesExpectedToWrite)
            }
        }
    }

    nonisolated func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask,
                                didFinishDownloadingTo location: URL) {
        let status = (downloadTask.response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200..<300).contains(status),
              downloadTask.countOfBytesReceived <= Self.maximumBytes else {
            Task { @MainActor in self.failed("ダウンロードに失敗しました（HTTP \(status)）。", session: session) }
            return
        }
        // URLSession deletes `location` after this delegate returns.
        let destination = FileManager.default.temporaryDirectory
            .appendingPathComponent("horizon-\(UUID().uuidString).zip")
        do {
            try FileManager.default.moveItem(at: location, to: destination)
            Task { @MainActor in self.accept(destination, from: session) }
        } catch {
            Task { @MainActor in self.failed("ZIPを保存できません: \(error.localizedDescription)", session: session) }
        }
    }

    nonisolated func urlSession(_ session: URLSession, task: URLSessionTask,
                                didCompleteWithError error: Error?) {
        guard let error else { return }
        Task { @MainActor in
            self.failed("ダウンロードが中断されました: \(error.localizedDescription)", session: session)
        }
    }

    nonisolated func urlSession(_ session: URLSession, task: URLSessionTask,
                                willPerformHTTPRedirection response: HTTPURLResponse,
                                newRequest request: URLRequest,
                                completionHandler: @escaping (URLRequest?) -> Void) {
        guard let url = request.url, url.scheme?.lowercased() == "https",
              url.user == nil, url.password == nil else {
            completionHandler(nil)
            return
        }
        completionHandler(request)
    }
}

/// Launcher-specific settings read/write through the existing Rust options
/// parser; they persist in this shell's app container and apply on next start.
private enum LauncherSettingsBridge {
    static func read() -> [String: Any]? {
        let size = horizon_ios_launcher_settings_json(nil, 0)
        guard size > 0, size <= 8192 else { return nil }
        var bytes = [UInt8](repeating: 0, count: size)
        let written = bytes.withUnsafeMutableBufferPointer { buffer in
            horizon_ios_launcher_settings_json(buffer.baseAddress, buffer.count)
        }
        guard written == size else { return nil }
        return (try? JSONSerialization.jsonObject(with: Data(bytes))) as? [String: Any]
    }

    static func set(_ key: String, _ value: String) -> Bool {
        let name = Array(key.utf8), text = Array(value.utf8)
        return name.withUnsafeBufferPointer { k in
            text.withUnsafeBufferPointer { v in
                horizon_ios_launcher_set_setting(k.baseAddress, k.count, v.baseAddress, v.count) == 0
            }
        }
    }

    static func setPlayer(_ name: String) -> Bool {
        let bytes = Array(name.utf8)
        return bytes.withUnsafeBufferPointer {
            horizon_ios_launcher_set_player_name($0.baseAddress, $0.count) == 0
        }
    }
}

struct LauncherView: View {
    let version: String
    let installed: Bool
    let importFromFiles: () -> Void

    @StateObject private var download = PreparedResourceDownload()
    @State private var link = ""
    @State private var checksum = ""
    @State private var showSettings = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Horizon")
                        .font(.largeTitle.bold())
                    Text("Minecraft: Java Edition \(version)")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button { showSettings = true } label: {
                    Image(systemName: "gearshape")
                        .font(.title3)
                        .accessibilityLabel("ゲーム設定")
                }
                .buttonStyle(.bordered)
            }

            Label(installed ? "ゲームデータ: インストール済み" : "ゲームデータ: 未インストール",
                  systemImage: installed ? "checkmark.circle" : "arrow.down.circle")
                .font(.subheadline.weight(.medium))

            Text("Windowsで準備した自分のゲームファイルを導入できます。HTTPS経由で取得する場合はZIPのURLとSHA-256を指定してください。")
                .font(.footnote)
                .foregroundStyle(.secondary)

            if !installed {
                Button("FilesからZIPを読み込む", action: importFromFiles)
                    .buttonStyle(.borderedProminent)
                    .disabled(download.isBusy)
                TextField("準備済みZIPのHTTPS URL", text: $link)
                    .textContentType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .textFieldStyle(.roundedBorder)
                    .accessibilityLabel("ゲームファイルのダウンロードURL")
                TextField("SHA-256（64桁）", text: $checksum)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .textFieldStyle(.roundedBorder)
                    .accessibilityLabel("ダウンロードファイルのSHA-256")

                HStack {
                    Button("ダウンロードしてインストール") {
                        download.start(link: link, checksum: checksum)
                    }
                    .buttonStyle(.bordered)
                    .disabled(download.isBusy)
                    if download.isBusy && !download.isInstalling {
                        Button("中止", role: .cancel) { download.cancel() }
                            .buttonStyle(.bordered)
                    }
                }
            }
            if download.isBusy {
                if let fraction = download.progress, !download.isInstalling {
                    ProgressView(value: fraction)
                } else {
                    ProgressView()
                }
            }
            if !download.status.isEmpty {
                Text(download.status)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
            Text("これはSwiftUIの開発用ランチャーです。ゲーム本体は別のアプリなので、インストール済みデータや設定はまだ自動共有されません。")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .sheet(isPresented: $showSettings) { LauncherSettingsView() }
    }
}

private struct LauncherSettingsView: View {
    @Environment(\.dismiss) private var dismiss
    @State private var playerName = ""
    @State private var renderDistance = 8
    @State private var simulationDistance = 8
    @State private var maxFPS = 60
    @State private var fov = 70
    @State private var volume = 1.0
    @State private var autoJump = false
    @State private var vsync = true
    @State private var message = ""

    var body: some View {
        NavigationStack {
            Form {
                Section("プレイヤー") {
                    TextField("オフライン名", text: $playerName)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    Text("3〜16文字の半角英数字・アンダースコア。オンライン認証とは別の設定です。")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Section("画面") {
                    Stepper("描画距離: \(renderDistance) chunks", value: $renderDistance, in: 2...24)
                    Stepper("シミュレーション距離: \(simulationDistance) chunks", value: $simulationDistance, in: 5...16)
                    Stepper("最大FPS: \(maxFPS)", value: $maxFPS, in: 30...120, step: 10)
                    Stepper("FOV: \(fov)", value: $fov, in: 30...110, step: 5)
                    Toggle("垂直同期", isOn: $vsync)
                }
                Section("操作・音声") {
                    Toggle("自動ジャンプ", isOn: $autoJump)
                    HStack {
                        Text("音量")
                        Slider(value: $volume, in: 0...1, step: 0.05)
                        Text("\(Int(volume * 100))%")
                            .monospacedDigit()
                    }
                }
                if !message.isEmpty {
                    Section { Text(message).font(.footnote).textSelection(.enabled) }
                }
                Section {
                    Text("Horizonのoptions.txtとhorizon-player.txtに保存します。変更は次回起動時に反映されます。独立したゲームIPAとは現在保存領域が異なります。")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            }
            .navigationTitle("ゲーム設定")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("閉じる") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("保存", action: save)
                }
            }
            .onAppear(perform: load)
        }
    }

    private func load() {
        guard let options = LauncherSettingsBridge.read() else {
            message = "設定を読み込めませんでした。"
            return
        }
        playerName = horizon_ios_player_name().map { String(cString: $0) } ?? ""
        renderDistance = (options["renderDistance"] as? Int) ?? 8
        simulationDistance = (options["simulationDistance"] as? Int) ?? 8
        maxFPS = (options["maxFps"] as? Int) ?? 60
        fov = (options["fov"] as? Int) ?? 70
        volume = (options["soundCategory_master"] as? Double) ?? 1
        autoJump = (options["autoJump"] as? Bool) ?? false
        vsync = (options["enableVsync"] as? Bool) ?? true
    }

    private func save() {
        let fields: [(String, String)] = [
            ("renderDistance", String(renderDistance)),
            ("simulationDistance", String(simulationDistance)),
            ("maxFps", String(maxFPS)),
            ("fov", String(fov)),
            ("soundCategory_master", String(format: "%.2f", volume)),
            ("autoJump", String(autoJump)),
            ("enableVsync", String(vsync)),
        ]
        guard fields.allSatisfy({ LauncherSettingsBridge.set($0.0, $0.1) }) else {
            message = "一部の設定を保存できませんでした。値を確認してください。"
            return
        }
        guard LauncherSettingsBridge.setPlayer(playerName) else {
            message = "プレイヤー名が無効です（3〜16文字の半角英数字・アンダースコア）。"
            return
        }
        message = "保存しました。アプリを再起動すると反映されます。"
    }
}
