import Foundation
import Security
import SwiftUI
import UIKit

/// Device-code sign-in uses the user's external browser, never a password field
/// or an embedded web view. Only Minecraft Java owners can create a session.
/// Tokens are stored in the device Keychain, not in Documents or UserDefaults.
@MainActor
final class MinecraftAccount: ObservableObject {
    @Published private(set) var name: String?
    @Published private(set) var status = ""
    @Published private(set) var userCode: String?
    @Published private(set) var verificationURL: URL?
    @Published private(set) var busy = false

    private let service = "dev.playhorizon.ios.minecraft"
    private let key = "minecraft-account"
    private let microsoft = "https://login.microsoftonline.com/consumers/oauth2/v2.0"
    private let minecraft = "https://api.minecraftservices.com"
    private var signInTask: Task<Void, Never>?

    private struct Credentials: Codable {
        var refreshToken: String
        var minecraftToken: String
        var expiresAt: Date
        var uuid: String
        var name: String
        var xuid: String?
    }

    private struct MicrosoftToken {
        let access: String
        let refresh: String
    }

    private struct DeviceCode {
        let code: String
        let userCode: String
        let verificationURL: URL
        let interval: Int
        let expiresAt: Date
    }

    private struct AccountError: LocalizedError {
        let errorDescription: String?
        init(_ message: String) { errorDescription = message }
    }

    var hasAccount: Bool { name != nil }
    var configured: Bool { clientID != nil }

    /// A dedicated, approved public-client registration is required; never
    /// borrow a different launcher's client ID or put a client secret in IPA.
    private var clientID: String? {
        guard let raw = Bundle.main.object(forInfoDictionaryKey: "HorizonMicrosoftClientID") as? String else { return nil }
        let value = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        let pattern = "^[0-9a-fA-F]{8}(-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}$"
        return value.range(of: pattern, options: .regularExpression) != nil ? value : nil
    }

    init() {
        if let saved = try? load() { name = saved.name }
    }

    func beginSignIn() {
        guard let clientID else {
            status = "配布者によるMicrosoftアプリ登録が未設定です。HorizonMicrosoftClientIDを設定してください。"
            return
        }
        guard !busy else { return }
        busy = true
        status = "Microsoftの認証コードを取得中…"
        signInTask = Task {
            do {
                let device = try await deviceCode(clientID: clientID)
                try Task.checkCancellation()
                userCode = device.userCode
                verificationURL = device.verificationURL
                UIPasteboard.general.string = device.userCode
                status = "ブラウザでコードを入力してください（コードをコピーしました）。"
                let tokens = try await poll(device: device, clientID: clientID)
                status = "Minecraft Java Editionの所有権を確認中…"
                let verified = try await minecraftSession(ms: tokens)
                try Task.checkCancellation()
                try save(verified)
                name = verified.name
                userCode = nil
                verificationURL = nil
                status = "\(verified.name) でログインしました。Java Editionの所有権を確認済みです。"
            } catch is CancellationError {
                status = "認証を中止しました。"
            } catch {
                status = error.localizedDescription
            }
            userCode = nil
            verificationURL = nil
            busy = false
            signInTask = nil
        }
    }

    func openVerification() {
        guard let verificationURL else { return }
        if let userCode { UIPasteboard.general.string = userCode }
        UIApplication.shared.open(verificationURL)
    }

    func cancelSignIn() {
        signInTask?.cancel()
        userCode = nil
        verificationURL = nil
    }

    func signOut() {
        cancelSignIn()
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                    kSecAttrService as String: service,
                                    kSecAttrAccount as String: key]
        SecItemDelete(query as CFDictionary)
        name = nil
        status = "ログアウトしました。"
    }

    /// Ownership gate for downloads of original Mojang assets. Both the main
    /// installer and the optional diagnostic downloader verify the account;
    /// the downloader itself never receives any access tokens.
    func verifyForDownload() async throws {
        _ = try await sessionForLaunch()
    }

    /// Refresh from Keychain and verify ownership on every game launch.
    /// Access tokens travel to Rust only via the in-process Swift bridge.
    func sessionForLaunch() async throws -> String {
        guard let clientID else { throw AccountError("Microsoftアプリ登録が未設定です。") }
        guard var saved = try load() else { throw AccountError("Microsoftアカウントでログインしてください。") }
        if saved.expiresAt <= Date().addingTimeInterval(3600) {
            status = "Microsoft認証を更新中…"
            let (code, reply) = try await form("\(microsoft)/token", [
                "client_id": clientID, "grant_type": "refresh_token",
                "refresh_token": saved.refreshToken, "scope": "XboxLive.signin offline_access"
            ])
            guard code == 200, let access = reply["access_token"] as? String,
                  let refresh = reply["refresh_token"] as? String else {
                throw AccountError("Microsoft認証の更新に失敗しました。再ログインしてください。")
            }
            saved = try await minecraftSession(ms: MicrosoftToken(access: access, refresh: refresh))
            try Task.checkCancellation()
            try save(saved)
            name = saved.name
        } else {
            try await verifyOwnership(token: saved.minecraftToken)
        }
        try Task.checkCancellation()
        let payload: [String: String] = [
            "name": saved.name, "uuid": saved.uuid,
            "access_token": saved.minecraftToken, "user_type": "msa",
            "xuid": saved.xuid ?? ""
        ]
        let bytes = try JSONSerialization.data(withJSONObject: payload)
        guard let json = String(data: bytes, encoding: .utf8) else { throw AccountError("セッションを生成できません。") }
        return json
    }

    private func deviceCode(clientID: String) async throws -> DeviceCode {
        let (status, response) = try await form("\(microsoft)/devicecode", [
            "client_id": clientID, "scope": "XboxLive.signin offline_access"
        ])
        guard status == 200, let code = response["device_code"] as? String,
              let user = response["user_code"] as? String,
              let rawURL = response["verification_uri"] as? String,
              let url = URL(string: rawURL), url.scheme == "https",
              let host = url.host?.lowercased(),
              (host == "microsoft.com" || host == "www.microsoft.com" || host == "login.microsoftonline.com"),
              let expiry = response["expires_in"] as? Int else {
            throw AccountError("Microsoft認証を開始できません。登録アプリとAPIアクセスを確認してください。")
        }
        return DeviceCode(code: code, userCode: user, verificationURL: url,
                          interval: max(1, min(30, response["interval"] as? Int ?? 5)),
                          expiresAt: Date().addingTimeInterval(TimeInterval(max(1, expiry))))
    }

    private func poll(device: DeviceCode, clientID: String) async throws -> MicrosoftToken {
        var interval = device.interval
        while Date() < device.expiresAt {
            try await Task.sleep(nanoseconds: UInt64(interval) * 1_000_000_000)
            try Task.checkCancellation()
            let (status, response) = try await form("\(microsoft)/token", [
                "client_id": clientID, "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                "device_code": device.code
            ])
            if status == 200, let access = response["access_token"] as? String,
               let refresh = response["refresh_token"] as? String {
                return MicrosoftToken(access: access, refresh: refresh)
            }
            switch response["error"] as? String {
            case "authorization_pending": continue
            case "slow_down": interval = min(interval + 5, 60)
            case "authorization_declined", "access_denied": throw AccountError("Microsoftログインがキャンセルされました。")
            case "expired_token", "bad_verification_code": throw AccountError("認証コードの有効期限が切れました。")
            default: throw AccountError("Microsoft認証に失敗しました。ログインをやり直してください。")
            }
        }
        throw AccountError("認証コードの有効期限が切れました。")
    }

    private func minecraftSession(ms: MicrosoftToken) async throws -> Credentials {
        let (xblCode, xbl) = try await json("https://user.auth.xboxlive.com/user/authenticate", [
            "Properties": ["AuthMethod": "RPS", "SiteName": "user.auth.xboxlive.com",
                           "RpsTicket": "d=\(ms.access)"],
            "RelyingParty": "http://auth.xboxlive.com", "TokenType": "JWT"
        ])
        guard xblCode == 200, let xboxToken = xbl["Token"] as? String else {
            throw AccountError("Xbox Liveへのログインに失敗しました。")
        }
        let (xstsCode, xsts) = try await json("https://xsts.auth.xboxlive.com/xsts/authorize", [
            "Properties": ["SandboxId": "RETAIL", "UserTokens": [xboxToken]],
            "RelyingParty": "rp://api.minecraftservices.com/", "TokenType": "JWT"
        ])
        guard xstsCode == 200, let token = xsts["Token"] as? String,
              let claims = (xsts["DisplayClaims"] as? [String: Any])?["xui"] as? [[String: Any]],
              let first = claims.first, let uhs = first["uhs"] as? String else {
            let error = xsts["XErr"] as? Int ?? 0
            throw AccountError("Xbox Liveの認証に失敗しました（\(error)）。Xboxプロフィールやファミリー設定を確認してください。")
        }
        let identity = "XBL3.0 x=\(uhs);\(token)"
        let (launcherStatus, launcher) = try await json("\(minecraft)/launcher/login", [
            "xtoken": identity, "platform": "PC_LAUNCHER"
        ])
        let access: [String: Any]
        if launcherStatus == 200, launcher["access_token"] is String {
            access = launcher
        } else {
            let (status, fallback) = try await json("\(minecraft)/authentication/login_with_xbox", ["identityToken": identity])
            guard status == 200, fallback["access_token"] is String else {
                throw AccountError("Minecraft Servicesでの認証に失敗しました。アプリ登録の承認が必要な可能性があります。")
            }
            access = fallback
        }
        guard let mcToken = access["access_token"] as? String,
              let ttl = access["expires_in"] as? Int else { throw AccountError("Minecraftトークンが不正です。") }
        try await verifyOwnership(token: mcToken)
        let (profileStatus, profile) = try await request("\(minecraft)/minecraft/profile", method: "GET", bearer: mcToken)
        guard profileStatus == 200, let uuid = profile["id"] as? String,
              let playerName = profile["name"] as? String,
              uuid.replacingOccurrences(of: "-", with: "").count == 32,
              (3...16).contains(playerName.count),
              playerName.allSatisfy({ $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "_") }) else {
            throw AccountError("Minecraft Java Editionのプロフィールが見つかりません。")
        }
        return Credentials(refreshToken: ms.refresh, minecraftToken: mcToken,
                           expiresAt: Date().addingTimeInterval(TimeInterval(max(1, ttl))),
                           uuid: uuid, name: playerName, xuid: first["xid"] as? String)
    }

    private func verifyOwnership(token: String) async throws {
        let (code, result) = try await request("\(minecraft)/entitlements/mcstore", method: "GET", bearer: token)
        guard code == 200, let items = result["items"] as? [[String: Any]] else {
            throw AccountError("Minecraftの所有権を確認できませんでした。ネットワークを確認してください。")
        }
        guard items.contains(where: { ["game_minecraft", "product_minecraft"].contains($0["name"] as? String ?? "") }) else {
            throw AccountError("このMicrosoftアカウントにはMinecraft: Java Editionの所有権がありません。")
        }
    }

    private func form(_ address: String, _ parameters: [String: String]) async throws -> (Int, [String: Any]) {
        let body = parameters.keys.sorted().map { key in
            let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "-._~"))
            func encoded(_ value: String) -> String { value.addingPercentEncoding(withAllowedCharacters: allowed) ?? "" }
            return "\(encoded(key))=\(encoded(parameters[key] ?? ""))"
        }.joined(separator: "&")
        return try await request(address, method: "POST", contentType: "application/x-www-form-urlencoded",
                                 body: Data(body.utf8))
    }

    private func json(_ address: String, _ payload: [String: Any]) async throws -> (Int, [String: Any]) {
        let data = try JSONSerialization.data(withJSONObject: payload)
        return try await request(address, method: "POST", contentType: "application/json", body: data)
    }

    private func request(_ address: String, method: String, contentType: String? = nil,
                         body: Data? = nil, bearer: String? = nil) async throws -> (Int, [String: Any]) {
        guard let url = URL(string: address), url.scheme == "https" else { throw AccountError("不正な認証先です。") }
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.timeoutInterval = 30
        request.httpBody = body
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        if let contentType { request.setValue(contentType, forHTTPHeaderField: "Content-Type") }
        if let bearer { request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization") }
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse, data.count <= 1024 * 1024,
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw AccountError("認証サーバーの応答を読み取れませんでした。")
        }
        return (http.statusCode, object)
    }

    private func load() throws -> Credentials? {
        let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                    kSecAttrService as String: service,
                                    kSecAttrAccount as String: key,
                                    kSecReturnData as String: true,
                                    kSecMatchLimit as String: kSecMatchLimitOne]
        var result: CFTypeRef?
        let code = SecItemCopyMatching(query as CFDictionary, &result)
        if code == errSecItemNotFound { return nil }
        guard code == errSecSuccess, let bytes = result as? Data else {
            throw AccountError("Keychainからアカウントを取得できませんでした。")
        }
        return try JSONDecoder().decode(Credentials.self, from: bytes)
    }

    private func save(_ credentials: Credentials) throws {
        let data = try JSONEncoder().encode(credentials)
        let match: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                    kSecAttrService as String: service,
                                    kSecAttrAccount as String: key]
        let attributes: [String: Any] = [kSecValueData as String: data,
                                         kSecAttrAccessible as String: kSecAttrAccessibleWhenUnlockedThisDeviceOnly]
        let status = SecItemUpdate(match as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        if status == errSecItemNotFound {
            var create = match
            create.merge(attributes) { _, new in new }
            guard SecItemAdd(create as CFDictionary, nil) == errSecSuccess else {
                throw AccountError("Keychainにアカウントを保存できませんでした。")
            }
        } else if status != errSecSuccess {
            throw AccountError("Keychainの認証情報を更新できませんでした。")
        }
    }
}
