import CryptoKit
import Foundation

/// Downloads *unaltered* Java Edition files from Mojang's HTTPS hosts after
/// a verified account sign-in. This is source material for horizon-prepare,
/// NOT Horizon's derived registry/texture tables or a playable installation.
/// Mojang's version manifest and per-file SHA-1 hashes are checked before an
/// atomic move; repeating an interrupted transfer skips verified files.
@MainActor
final class OfficialMinecraftDownload: ObservableObject {
    @Published private(set) var busy = false
    @Published private(set) var progress = ""
    @Published private(set) var message = ""
    private var task: Task<Void, Never>?

    private struct Asset: Decodable {
        let hash: String
        let size: Int64
    }
    private struct AssetIndex: Decodable { let objects: [String: Asset] }
    private struct Package: Decodable {
        let url: URL
        let sha1: String
        let size: Int64
    }
    private struct Version: Decodable {
        struct Index: Decodable {
            let id: String
            let url: URL
            let sha1: String
            let size: Int64
        }
        struct Downloads: Decodable { let client: Package; let server: Package }
        struct Library: Decodable {
            struct Artifact: Decodable {
                let path: String
                let url: URL
                let sha1: String
                let size: Int64
            }
            struct Files: Decodable { let artifact: Artifact? }
            let downloads: Files?
        }
        let id: String
        let downloads: Downloads
        let assetIndex: Index
        let libraries: [Library]
    }
    private struct ReleaseManifest: Decodable {
        struct Entry: Decodable { let id: String; let url: URL; let sha1: String }
        let versions: [Entry]
    }
    private struct DownloadError: LocalizedError {
        let errorDescription: String?
        init(_ value: String) { errorDescription = value }
    }
    private static let hosts: Set<String> = [
        "piston-meta.mojang.com", "piston-data.mojang.com",
        "resources.download.minecraft.net", "libraries.minecraft.net"
    ]

    func start(version: String, account: MinecraftAccount) {
        guard !busy else { return }
        busy = true
        progress = ""
        message = "所有権を確認中…"
        task = Task {
            do {
                try await account.verifyForDownload()
                try await install(version: version)
                message = "公式ゲームデータを保存しました。Horizonゲーム用の前処理は、JDK 25を使うPCで別途必要です。"
            } catch is CancellationError {
                message = "ダウンロードを停止しました。次回は検証済みファイルを再利用します。"
            } catch {
                message = error.localizedDescription
            }
            busy = false
            task = nil
        }
    }

    func cancel() { task?.cancel() }

    private func install(version: String) async throws {
        guard version.range(of: "^[0-9A-Za-z._-]{1,32}$", options: .regularExpression) != nil,
              version != ".", version != ".." else { throw DownloadError("不正なMinecraftバージョンです。") }
        let documents = try FileManager.default.url(for: .documentDirectory, in: .userDomainMask,
                                                    appropriateFor: nil, create: true)
        let root = documents.appendingPathComponent("official-minecraft", isDirectory: true)
        let versions = root.appendingPathComponent("versions/\(version)", isDirectory: true)
        let (manifestData, _) = try await Self.metadata(
            URL(string: "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")!,
            maxSize: 8 * 1024 * 1024)
        let manifest = try JSONDecoder().decode(ReleaseManifest.self, from: manifestData)
        guard let entry = manifest.versions.first(where: { $0.id == version }) else {
            throw DownloadError("Mojangの公式バージョン一覧に\(version)がありません。")
        }
        let (versionData, versionURL) = try await Self.metadata(entry.url, maxSize: 8 * 1024 * 1024)
        try Self.verify(versionData, sha1: entry.sha1)
        let info = try JSONDecoder().decode(Version.self, from: versionData)
        guard info.id == version else { throw DownloadError("公式のバージョン情報が一致しません。") }
        try Self.writeMetadata(versionData, into: versions.appendingPathComponent("\(version).json"))

        guard Self.safeRelative(info.assetIndex.id) != nil,
              !info.assetIndex.id.contains("/") else {
            throw DownloadError("公式アセット一覧の名前が不正です。")
        }
        let indexURL = root.appendingPathComponent("assets/indexes/\(info.assetIndex.id).json")
        let (indexData, _) = try await Self.metadata(info.assetIndex.url, maxSize: 32 * 1024 * 1024)
        try Self.verify(indexData, sha1: info.assetIndex.sha1)
        guard Int64(indexData.count) == info.assetIndex.size else { throw DownloadError("アセット一覧のサイズが不正です。") }
        let index = try JSONDecoder().decode(AssetIndex.self, from: indexData)
        try Self.writeMetadata(indexData, into: indexURL)

        // The same folder layout as the Minecraft Java launcher. A prepared
        // Horizon install requires a separate Java exporter pass on a PC.
        var files: [(URL, URL, String, Int64)] = [
            (info.downloads.client.url, versions.appendingPathComponent("\(version).jar"),
             info.downloads.client.sha1, info.downloads.client.size),
            (info.downloads.server.url, root.appendingPathComponent("server.jar"),
             info.downloads.server.sha1, info.downloads.server.size)
        ]
        for library in info.libraries {
            guard let a = library.downloads?.artifact else { continue }
            guard let relative = Self.safeRelative(a.path) else {
                throw DownloadError("公式ライブラリのファイル名が不正です。")
            }
            files.append((a.url, root.appendingPathComponent("libraries").appendingPathComponent(relative),
                          a.sha1, a.size))
        }
        for asset in index.objects.values {
            let sha = asset.hash.lowercased()
            guard Self.validSHA1(sha) else { throw DownloadError("公式アセットのハッシュが不正です。") }
            let url = URL(string: "https://resources.download.minecraft.net/\(sha.prefix(2))/\(sha)")!
            let path = root.appendingPathComponent("assets/objects/\(sha.prefix(2))/\(sha)")
            files.append((url, path, sha, asset.size))
        }
        guard files.count <= 15000,
              files.allSatisfy({ $0.3 > 0 && $0.3 < 512 * 1024 * 1024 }),
              files.reduce(Int64(0), { $0 + $1.3 }) < 5 * 1024 * 1024 * 1024 else {
            throw DownloadError("公式ファイル一覧の容量が大きすぎます。")
        }
        for (number, item) in files.enumerated() {
            try Task.checkCancellation()
            progress = "公式データを検証・取得中 \(number + 1)/\(files.count)"
            try await Self.download(item.0, to: item.1, sha1: item.2, size: item.3)
        }
        progress = "\(files.count)ファイルを検証しました（\(versionURL.host ?? "Mojang")）"
    }

    /// Constrain every network download to known official hosts (even when the
    /// version manifest is malformed). No arbitrary CDN or attacker path.
    private static func trusted(_ url: URL) throws {
        guard url.scheme?.lowercased() == "https", let host = url.host?.lowercased(),
              hosts.contains(host), url.user == nil, url.password == nil,
              url.fragment == nil else { throw DownloadError("Mojang以外のダウンロード先を拒否しました。") }
    }

    private static func metadata(_ url: URL, maxSize: Int) async throws -> (Data, URL) {
        try trusted(url)
        var request = URLRequest(url: url)
        request.timeoutInterval = 40
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse, http.statusCode == 200,
              let destination = response.url, destination.host == url.host,
              data.count > 0, data.count <= maxSize else {
            throw DownloadError("Mojangの公式メタデータを取得できませんでした。")
        }
        return (data, destination)
    }

    private static func download(_ url: URL, to path: URL, sha1: String, size: Int64) async throws {
        try trusted(url)
        guard validSHA1(sha1), size > 0, size < 512 * 1024 * 1024 else {
            throw DownloadError("公式ダウンロードの検証情報が不正です。")
        }
        if FileManager.default.fileExists(atPath: path.path),
           (try? verifyFile(path, sha1: sha1, size: size)) == true { return }
        var request = URLRequest(url: url)
        request.timeoutInterval = 120
        let (temporary, response) = try await URLSession.shared.download(for: request)
        defer { try? FileManager.default.removeItem(at: temporary) }
        guard let http = response as? HTTPURLResponse, http.statusCode == 200,
              response.url?.host == url.host else {
            throw DownloadError("Mojangの公式データをダウンロードできませんでした。")
        }
        guard try verifyFile(temporary, sha1: sha1, size: size) else {
            throw DownloadError("公式データのSHA-1またはサイズが一致しません。")
        }
        try FileManager.default.createDirectory(at: path.deletingLastPathComponent(),
                                                withIntermediateDirectories: true)
        if FileManager.default.fileExists(atPath: path.path) { try FileManager.default.removeItem(at: path) }
        try FileManager.default.moveItem(at: temporary, to: path)
    }

    private static func writeMetadata(_ data: Data, into path: URL) throws {
        try FileManager.default.createDirectory(at: path.deletingLastPathComponent(),
                                                withIntermediateDirectories: true)
        try data.write(to: path, options: .atomic)
    }

    private static func validSHA1(_ value: String) -> Bool {
        value.count == 40 && value.utf8.allSatisfy { ($0 >= 48 && $0 <= 57) || ($0 >= 97 && $0 <= 102) }
    }
    private static func verify(_ data: Data, sha1: String) throws {
        guard validSHA1(sha1.lowercased()),
              Insecure.SHA1.hash(data: data).map({ String(format: "%02x", $0) }).joined() == sha1.lowercased() else {
            throw DownloadError("MojangのメタデータのSHA-1が一致しません。")
        }
    }
    private static func verifyFile(_ path: URL, sha1: String, size: Int64) throws -> Bool {
        let values = try path.resourceValues(forKeys: [.fileSizeKey])
        guard Int64(values.fileSize ?? -1) == size else { return false }
        guard let stream = InputStream(url: path) else { return false }
        stream.open()
        defer { stream.close() }
        var hasher = Insecure.SHA1()
        let buffer = UnsafeMutablePointer<UInt8>.allocate(capacity: 128 * 1024)
        defer { buffer.deallocate() }
        while stream.hasBytesAvailable {
            let count = stream.read(buffer, maxLength: 128 * 1024)
            if count < 0 { return false }
            if count == 0 { break }
            hasher.update(data: Data(bytes: buffer, count: count))
        }
        let hash = hasher.finalize().map { String(format: "%02x", $0) }.joined()
        return hash == sha1.lowercased()
    }
    private static func safeRelative(_ path: String) -> String? {
        let parts = path.split(separator: "/", omittingEmptySubsequences: false)
        guard !parts.isEmpty, parts.count <= 20, parts.allSatisfy({ part in
            !part.isEmpty && part != "." && part != ".." && part.utf8.allSatisfy({
                ($0 >= 48 && $0 <= 57) || ($0 >= 65 && $0 <= 90) ||
                ($0 >= 97 && $0 <= 122) || $0 == 45 || $0 == 46 || $0 == 95
            })
        }) else { return nil }
        return path
    }
}
