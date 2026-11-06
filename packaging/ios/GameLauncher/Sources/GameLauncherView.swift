import CryptoKit
import Foundation
import SwiftUI
import UIKit

/// The publisher supplies a JSON manifest URL once when making the app.
/// A configured publisher build needs no folder navigation or URL entry.
/// Players may also supply their own private HTTPS link if unconfigured.
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

/// One active transfer, either from a publisher-provided, integrity-pinned
/// manifest or from the player's own HTTPS ZIP link. Both use the same checked
/// Rust installer and the same Documents sandbox as the real game.
private struct GameDownloadSource {
    let url: URL
    let sha256: String?
    let size: Int64?
}

@MainActor
final class NativeGameDownload: NSObject, ObservableObject {
    @Published private(set) var message = ""
    @Published private(set) var busy = false
    @Published private(set) var pausing = false
    @Published private(set) var installing = false
    @Published private(set) var downloadedBytes: Int64 = 0
    @Published private(set) var totalBytes: Int64 = 0
    @Published private(set) var installed = false

    private var session: URLSession?
    private var activeTask: URLSessionDownloadTask?
    private var activeWasResumed = false
    private var source: GameDownloadSource?
    private var generation = UUID()
    private var backgroundTask: UIBackgroundTaskIdentifier = .invalid
    private nonisolated static let limit: Int64 = 2 * 1024 * 1024 * 1024

    private struct PausedDownload: Codable {
        let key: String
        let data: Data
    }

    private static var resumeFile: URL? {
        FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first?
            .appendingPathComponent("horizon-game-download-resume.plist")
    }

    private static func resumeKey(for source: GameDownloadSource) -> String {
        let identity = "\(source.url.absoluteString)\n\(source.sha256 ?? "")\n\(source.size.map { String($0) } ?? "")"
        return SHA256.hash(data: Data(identity.utf8))
            .map { String(format: "%02x", $0) }.joined()
    }

    private func saveResume(_ data: Data, key: String) {
        guard !data.isEmpty, data.count <= 4 * 1024 * 1024,
              let url = Self.resumeFile,
              let encoded = try? PropertyListEncoder().encode(PausedDownload(key: key, data: data))
        else { return }
        try? encoded.write(to: url, options: .atomic)
    }

    private func loadResume(for source: GameDownloadSource) -> Data? {
        guard let url = Self.resumeFile,
              let saved = try? PropertyListDecoder().decode(PausedDownload.self, from: Data(contentsOf: url)),
              saved.key == Self.resumeKey(for: source), saved.data.count <= 4 * 1024 * 1024
        else { return nil }
        return saved.data
    }

    private func clearResume() {
        if let url = Self.resumeFile { try? FileManager.default.removeItem(at: url) }
    }

    private static func trustedHTTPS(_ text: String) -> URL? {
        guard let url = URL(string: text.trimmingCharacters(in: .whitespacesAndNewlines)),
              url.scheme?.lowercased() == "https", url.host != nil,
              url.user == nil, url.password == nil, url.fragment == nil
        else { return nil }
        return url
    }

    var progress: Double? {
        totalBytes > 0 ? min(1, Double(downloadedBytes) / Double(totalBytes)) : nil
    }

    /// Prefer the publisher's pinned manifest. No third-party game-file mirror
    /// is silently trusted or bundled with the application.
    func begin(version: String) async {
        guard !busy else { return }
        guard let address = Bundle.main.object(forInfoDictionaryKey: "HorizonResourcesManifestURL") as? String,
              let manifestURL = Self.trustedHTTPS(address) else {
            message = "ゲームファイルの配信元が設定されていません。配布元の設定が必要です。下のHTTPSリンクから自分のZIPを取得することもできます。"
            return
        }
        let id = UUID()
        generation = id
        busy = true
        installing = false
        downloadedBytes = 0
        totalBytes = 0
        message = "配信情報を確認中…"
        let current = URLSession(configuration: .default, delegate: self, delegateQueue: nil)
        session = current
        do {
            var request = URLRequest(url: manifestURL)
            request.timeoutInterval = 30
            request.setValue("application/json", forHTTPHeaderField: "Accept")
            let (data, response) = try await current.data(for: request)
            guard generation == id, session === current else { return }
            guard (response as? HTTPURLResponse)?.statusCode == 200, data.count <= 32 * 1024 else {
                throw DownloadProblem("配信情報を取得できませんでした。")
            }
            let manifest = try JSONDecoder().decode(GameFilesManifest.self, from: data)
            try manifest.validate(expectedVersion: version)
            startDownload(GameDownloadSource(url: manifest.url, sha256: manifest.sha256.lowercased(),
                                             size: manifest.size), on: current)
        } catch {
            fail(error.localizedDescription, from: current)
        }
    }

    /// The fallback from the reference launcher: the player can host their own
    /// legally prepared ZIP without selecting folders or editing Info.plist.
    /// A checksum is optional for personal links, but strongly recommended.
    func beginFromLink(_ text: String, checksum: String) {
        guard !busy else { return }
        guard let url = Self.trustedHTTPS(text) else {
            message = "自分で用意したZIPのHTTPSリンクを入力してください。"
            return
        }
        let normalized = checksum.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard normalized.isEmpty || (normalized.count == 64 && normalized.utf8.allSatisfy({
            ($0 >= 48 && $0 <= 57) || ($0 >= 97 && $0 <= 102)
        })) else {
            message = "SHA-256は64桁の16進数で入力してください。"
            return
        }
        generation = UUID()
        busy = true
        installing = false
        downloadedBytes = 0
        totalBytes = 0
        let current = URLSession(configuration: .default, delegate: self, delegateQueue: nil)
        session = current
        startDownload(GameDownloadSource(url: url, sha256: normalized.isEmpty ? nil : normalized,
                                         size: nil), on: current)
    }

    private func startDownload(_ source: GameDownloadSource, on current: URLSession) {
        self.source = source
        totalBytes = source.size ?? 0
        let paused = loadResume(for: source)
        if paused == nil { clearResume() }
        if paused != nil {
            message = "中断したダウンロードを再開中…"
        } else if source.sha256 == nil {
            message = "ZIPをダウンロード中…（SHA-256未指定。インストール時にファイル構造を検証します）"
        } else {
            message = "ゲームファイルをダウンロード中…"
        }
        if backgroundTask == .invalid {
            backgroundTask = UIApplication.shared.beginBackgroundTask(withName: "Horizon game download") { [weak self] in
                // iOS grants finite background time. Persist resume data rather
                // than losing the partial transfer when the app is suspended.
                Task { @MainActor in self?.cancel() }
            }
        }
        let task = paused.map { current.downloadTask(withResumeData: $0) }
            ?? current.downloadTask(with: URLRequest(url: source.url))
        activeWasResumed = paused != nil
        activeTask = task
        task.resume()
    }

    func cancel() {
        guard busy && !installing && !pausing else { return }
        generation = UUID()
        let current = session
        let task = activeTask
        let currentSource = source
        session = nil
        activeTask = nil
        source = nil
        message = "ダウンロードを中断中…"
        if let task, let currentSource {
            // The resume-data callback is asynchronous. Do not enable a new
            // download until the old partial state is saved to disk.
            pausing = true
            let key = Self.resumeKey(for: currentSource)
            task.cancel(byProducingResumeData: { [weak self] data in
                Task { @MainActor in
                    if let self {
                        if let data {
                            self.saveResume(data, key: key)
                        } else {
                            self.clearResume()
                        }
                        self.pausing = false
                        self.busy = false
                        self.message = "ダウンロードを中断しました。再開できる場合は次回続きから取得します。"
                        self.endBackground()
                    }
                    current?.finishTasksAndInvalidate()
                }
            })
        } else {
            busy = false
            current?.invalidateAndCancel()
            endBackground()
        }
    }

    private func endBackground() {
        if backgroundTask != .invalid {
            UIApplication.shared.endBackgroundTask(backgroundTask)
            backgroundTask = .invalid
        }
    }

    private func fail(_ reason: String, from current: URLSession) {
        guard session === current else { return }
        generation = UUID()
        session = nil
        activeTask = nil
        source = nil
        current.invalidateAndCancel()
        busy = false
        installing = false
        message = reason
        endBackground()
    }

    fileprivate func finishDownload(_ archive: URL, from current: URLSession) {
        guard session === current, let source else {
            try? FileManager.default.removeItem(at: archive)
            return
        }
        activeTask = nil
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
                if let expected = source.size, length != expected {
                    throw DownloadProblem("ダウンロードサイズが一致しません。再試行してください。")
                }
                if let expected = source.sha256, actual != expected {
                    throw DownloadProblem("ダウンロードしたデータのSHA-256が一致しません。")
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
                    self.clearResume()
                    try? FileManager.default.removeItem(at: archive)
                    self.fail(error.localizedDescription, from: current)
                case .success:
                    self.clearResume()
                    self.message = "ファイルをインストール中…"
                    let answer = await Bridge.send("launcher-import \(archive.lastPathComponent)")
                    try? FileManager.default.removeItem(at: archive)
                    guard self.session === current, self.generation == id else { return }
                    if let issue = answer.error {
                        self.fail(issue, from: current)
                    } else {
                        self.session = nil
                        self.source = nil
                        current.finishTasksAndInvalidate()
                        self.busy = false
                        self.installing = false
                        self.endBackground()
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
                self.clearResume()
                self.fail("ダウンロードが上限の2GiBを超えました。", from: session)
            } else {
                self.downloadedBytes = totalBytesWritten
                self.totalBytes = self.source?.size ?? max(totalBytesExpectedToWrite, 0)
            }
        }
    }

    nonisolated func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask,
                                didResumeAtOffset fileOffset: Int64, expectedTotalBytes: Int64) {
        Task { @MainActor in
            guard self.session === session else { return }
            self.downloadedBytes = fileOffset
            self.totalBytes = self.source?.size ?? max(expectedTotalBytes, 0)
        }
    }

    nonisolated func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask,
                                didFinishDownloadingTo location: URL) {
        let code = (downloadTask.response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200..<300).contains(code), downloadTask.countOfBytesReceived <= Self.limit else {
            Task { @MainActor in
                self.clearResume()
                self.fail("ダウンロードに失敗しました (HTTP \(code))。", from: session)
            }
            return
        }
        // Move inside the delegate: iOS deletes the temporary file on return.
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
        let data = (error as NSError).userInfo[NSURLSessionDownloadTaskResumeData] as? Data
        Task { @MainActor in
            guard self.session === session else { return }
            if let data, let source = self.source {
                self.saveResume(data, key: Self.resumeKey(for: source))
                self.fail("通信が切れました。もう一度押すと途中から再開します。", from: session)
            } else if self.activeWasResumed {
                // A resumed task that cannot continue returns no replacement
                // resume data. Purge the stale token so retry downloads anew.
                self.clearResume()
                self.fail("再開データを使えませんでした。もう一度押すと最初からダウンロードします。", from: session)
            } else {
                self.fail(error.localizedDescription, from: session)
            }
        }
    }

    nonisolated func urlSession(_ session: URLSession, task: URLSessionTask,
                                willPerformHTTPRedirection response: HTTPURLResponse,
                                newRequest request: URLRequest,
                                completionHandler: @escaping (URLRequest?) -> Void) {
        guard let url = request.url, url.scheme?.lowercased() == "https", url.host != nil,
              url.user == nil, url.password == nil, url.fragment == nil else {
            completionHandler(nil)
            return
        }
        completionHandler(request)
    }
}

/// iOS 26 uses the system Liquid Glass material; older OS versions retain
/// the native translucent material instead of a custom fake glass shader.
private extension View {
    @ViewBuilder func horizonGlassPanel() -> some View {
        if #available(iOS 26.0, *) {
            self.padding(18).glassEffect(.regular, in: .rect(cornerRadius: 26))
        } else {
            self.padding(18).background(.regularMaterial, in: RoundedRectangle(cornerRadius: 26))
        }
    }
}

struct GameLauncherView: View {
    let onPlay: () -> Void
    @StateObject private var download = NativeGameDownload()
    @StateObject private var account = MinecraftAccount()
    @StateObject private var official = OfficialMinecraftDownload()
    @State private var hasResources = false
    @State private var hasEngineData = false
    @State private var didLoadInstallState = false
    @State private var version = "26.3"
    @State private var notice = ""
    @State private var showSettings = false
    @State private var starting = false
    @State private var customLink = ""
    @State private var customSHA256 = ""
    @State private var showPersonalLink = false
    @State private var showDiagnosticDownload = false
    @State private var nativeDownloading = false
    @State private var nativeDownloadStage = ""
    @State private var nativeDownloadDone: Int64 = 0
    @State private var nativeDownloadTotal: Int64 = 0

    private var hasConfiguredPublisher: Bool {
        guard let text = Bundle.main.object(forInfoDictionaryKey: "HorizonResourcesManifestURL") as? String,
              let url = URL(string: text), url.scheme?.lowercased() == "https", url.host != nil,
              url.user == nil, url.password == nil else { return false }
        return true
    }

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
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Microsoftアカウント").font(.title3.bold())
                        if let name = account.name {
                            Label("ログイン済み: \(name)", systemImage: "person.crop.circle.badge.checkmark")
                            Button("ログアウト") { account.signOut() }.buttonStyle(.bordered)
                        } else if account.busy {
                            ProgressView("Microsoftアカウントを認証中…")
                        }
                        if !account.hasAccount && !account.busy {
                            Button("Microsoftアカウントでログイン") { account.beginSignIn() }
                                .buttonStyle(.borderedProminent)
                                .disabled(!account.configured)
                            if !account.configured {
                                Text("Microsoft認証の設定が未完了です。ログイン・ゲームデータのダウンロード・プレイには、アプリのクライアントID設定が必要です。")
                                    .font(.footnote).foregroundStyle(.secondary)
                            }
                        }
                        if let code = account.userCode {
                            Text("認証コード: \(code)").font(.headline.monospaced())
                            Button("Microsoft認証ページを開く（コードをコピー）") { account.openVerification() }
                            Button("認証を中止") { account.cancelSignIn() }
                        }
                        if !account.status.isEmpty {
                            Text(account.status).font(.footnote).foregroundStyle(.secondary)
                        }
                    }.horizonGlassPanel()
                    if hasResources {
                        Label("ゲームファイルを準備できました", systemImage: "checkmark.circle.fill")
                            .foregroundStyle(.green)
                        Button { Task { await startGame() } } label: {
                            Label("プレイ", systemImage: "play.fill")
                                .frame(maxWidth: .infinity, minHeight: 40)
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(starting || !account.hasAccount || !account.configured)
                        .horizonGlassPanel()
                        if !account.hasAccount {
                            Text("プレイにはMicrosoftアカウントへのログインとMinecraft: Java Editionの所有権確認が必要です。")
                                .font(.footnote).foregroundStyle(.secondary)
                        }
                    } else {
                        VStack(alignment: .leading, spacing: 12) {
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
                        if nativeDownloading {
                            ProgressView(nativeDownloadStage.isEmpty ? "準備中…" : nativeDownloadStage)
                            if nativeDownloadTotal > 0 {
                                ProgressView(value: Double(nativeDownloadDone), total: Double(nativeDownloadTotal))
                                Text("\(nativeDownloadDone / 1_048_576) / \(nativeDownloadTotal / 1_048_576) MiB")
                                    .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
                            }
                        } else {
                            Button { Task { await downloadFromMojang() } } label: {
                                Label("Mojang公式からダウンロードしてインストール", systemImage: "arrow.down.circle")
                                    .frame(maxWidth: .infinity, minHeight: 40)
                            }.buttonStyle(.borderedProminent)
                                .disabled(!account.hasAccount || !account.configured || account.busy || download.busy || official.busy)
                            if didLoadInstallState && !hasEngineData {
                                Text("このビルドには生成済みエンジンテーブルが含まれていません。ダウンロード時にエラーが出る場合は、最新のgame IPAを使用してください。")
                                    .font(.footnote).foregroundStyle(.secondary)
                            }
                        }
                        if !account.hasAccount {
                            Text("ダウンロードするにはMicrosoftアカウントでログインし、Minecraft Java Editionの所有権を確認してください。配布先URLの設定は不要です。")
                                .font(.footnote).foregroundStyle(.secondary)
                        }
                        // Optional publisher-provided ZIP source: it is not required
                        // for the built-in Mojang installer above. Do not suggest
                        // that the app is unconfigured when engine-data.zip exists.
                        if download.busy {
                            if download.pausing {
                                Button("中断中…") {}.disabled(true)
                            } else if !download.installing {
                                Button("キャンセル", role: .cancel) { download.cancel() }
                                    .buttonStyle(.bordered)
                            }
                        } else if !nativeDownloading && hasConfiguredPublisher {
                            Button { Task { await download.begin(version: version) } } label: {
                                Label("配布元の準備済みZIPをダウンロード", systemImage: "archivebox")
                                    .frame(maxWidth: .infinity, minHeight: 40)
                            }.buttonStyle(.bordered)
                                .disabled(!account.hasAccount)
                        }
                        Divider()
                        DisclosureGroup("元の公式ファイルだけを取得（診断用）", isExpanded: $showDiagnosticDownload) {
                            Text("上のインストーラーはAndroid版と共通のRust処理を利用します。こちらの診断用ダウンロードで取得した元ファイルのみでは起動できず、JDK 25搭載PCでのhorizon-prepare処理が別途必要です。")
                                .font(.footnote).foregroundStyle(.secondary)
                            if official.busy {
                                ProgressView()
                                Text(official.progress).font(.caption.monospacedDigit())
                                Button("公式データ取得を中止") { official.cancel() }
                            } else {
                                Button("Mojang公式ゲームデータを取得（診断用）") {
                                    official.start(version: version, account: account)
                                }
                                .disabled(!account.hasAccount)
                            }
                            if !official.message.isEmpty {
                                Text(official.message).font(.footnote).foregroundStyle(.secondary)
                            }
                        }
                        DisclosureGroup("自分で用意したZIPからインストール", isExpanded: $showPersonalLink) {
                            Text("自分のMinecraft Java Editionから正規に準備したZIPを、HTTPSで公開された個人用リンクから取得できます。フォルダ操作は不要です。")
                                .font(.footnote).foregroundStyle(.secondary)
                            TextField("https://example.com/horizon-26.3.zip", text: $customLink)
                                .keyboardType(.URL)
                                .textInputAutocapitalization(.never)
                                .autocorrectionDisabled()
                            TextField("SHA-256（任意・推奨）", text: $customSHA256)
                                .textInputAutocapitalization(.never)
                                .autocorrectionDisabled()
                            Button("このリンクからダウンロード") {
                                download.beginFromLink(customLink, checksum: customSHA256)
                            }
                            .disabled(download.busy || !account.hasAccount || customLink.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                        }
                        }.horizonGlassPanel()
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
        .task {
            await refresh()
            // Show the manual ZIP fallback only when neither automatic route exists.
            if !hasEngineData && !hasConfiguredPublisher { showPersonalLink = true }
        }
        .onChange(of: download.installed) { installed in
            if installed { Task { await refresh() } }
        }
    }

    private func downloadFromMojang() async {
        guard !nativeDownloading && !download.busy && !official.busy else { return }
        guard account.hasAccount && account.configured else {
            notice = "ダウンロードするにはMicrosoftアカウントでログインしてください。"
            return
        }
        // Verify ownership here as well as in Play, not just through the UI
        // button's disabled state. No access token goes to the downloader.
        do {
            _ = try await account.sessionForLaunch()
        } catch {
            notice = error.localizedDescription
            return
        }
        notice = ""
        nativeDownloading = true
        nativeDownloadDone = 0
        nativeDownloadTotal = 0
        let pending = Task { await Bridge.send("launcher-download") }
        // Keep UI polling asynchronous: the installer runs on a Rust worker.
        let poller = Task {
            while !Task.isCancelled {
                let state = await Bridge.send("launcher-download-state")
                nativeDownloadStage = state["stage"] as? String ?? ""
                nativeDownloadDone = (state["done"] as? NSNumber)?.int64Value ?? 0
                nativeDownloadTotal = (state["total"] as? NSNumber)?.int64Value ?? 0
                try? await Task.sleep(nanoseconds: 350_000_000)
            }
        }
        let reply = await pending.value
        poller.cancel()
        nativeDownloading = false
        if let error = reply.error { notice = error }
        else { await refresh() }
    }

    private func refresh() async {
        let status = await Bridge.send("launcher-state")
        if let version = status["version"] as? String { self.version = version }
        hasResources = status["ready"] as? Bool ?? false
        hasEngineData = status["engine_data"] as? Bool ?? false
        if let issue = status.error { notice = issue }
        else { didLoadInstallState = true }
    }

    private func startGame() async {
        guard !starting else { return }
        starting = true
        do {
            let session = try await account.sessionForLaunch()
            let delivered = await Bridge.send("launcher-session \(session)")
            if let error = delivered.error { throw NSError(domain: "Horizon", code: 1,
                                                           userInfo: [NSLocalizedDescriptionKey: error]) }
        } catch {
            notice = error.localizedDescription
            starting = false
            return
        }
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
