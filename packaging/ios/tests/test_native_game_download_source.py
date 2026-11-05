"""Native single-app launcher: link fallback, safe verification and resumable transfer."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[3]
VIEW = ROOT / 'packaging/ios/GameLauncher/Sources/GameLauncherView.swift'


class NativeGameDownloadContractTests(unittest.TestCase):
    def test_player_owned_https_archive_can_install_without_publisher_manifest(self):
        swift = VIEW.read_text()
        self.assertIn('func beginFromLink(', swift)
        self.assertIn('private static func trustedHTTPS', swift)
        self.assertIn('DisclosureGroup("自分で用意したZIPからインストール"', swift)
        self.assertIn('TextField("https://', swift)
        self.assertIn('download.beginFromLink(', swift)
        self.assertIn('Bridge.send("launcher-import ', swift)
        self.assertIn('ゲームファイルの配信元が設定されていません', swift)
        self.assertNotIn('cdn.gayfeet.lol', swift)

    def test_unconfigured_app_exposes_fallback_instead_of_an_unusable_main_button(self):
        swift = VIEW.read_text()
        self.assertIn('private var hasConfiguredPublisher: Bool', swift)
        self.assertIn('isExpanded: $showPersonalLink', swift)
        self.assertIn('if !hasEngineData && !hasConfiguredPublisher { showPersonalLink = true }', swift)
        self.assertIn('} else if !nativeDownloading && hasConfiguredPublisher {', swift)
        self.assertNotIn('このビルドには自動配信先が設定されていません', swift)

    def test_builtin_mojang_download_is_independent_of_publisher_manifest(self):
        swift = VIEW.read_text()
        self.assertIn('Label("Mojang公式からダウンロードしてインストール"', swift)
        self.assertIn('Task { await downloadFromMojang() }', swift)
        self.assertIn('Bridge.send("launcher-download")', swift)
        self.assertIn('hasEngineData = status["engine_data"] as? Bool ?? false', swift)
        main = swift.index('Label("Mojang公式からダウンロードしてインストール"')
        optional = swift.index('} else if !nativeDownloading && hasConfiguredPublisher {')
        self.assertLess(main, optional)
        self.assertIn('if download.busy {\n                            if download.pausing {', swift)
        self.assertNotIn('このビルドには自動配信先が設定されていません', swift)

    def test_builtin_official_download_does_not_require_microsoft_app_registration(self):
        swift = VIEW.read_text()
        main = swift.index('Label("Mojang公式からダウンロードしてインストール"')
        next_section = swift.index('// Optional publisher-provided ZIP source:', main)
        button = swift[main:next_section]
        self.assertIn('.disabled(download.busy || official.busy)', button)
        self.assertNotIn('!account.hasAccount', button.split('.disabled(', 1)[1].split(')', 1)[0])
        self.assertNotIn('!hasEngineData', button.split('.disabled(', 1)[1].split(')', 1)[0])
        self.assertNotIn('hasConfiguredPublisher', button)

        download = swift.split('private func downloadFromMojang() async {', 1)[1].split('private func refresh() async {', 1)[0]
        self.assertIn('Bridge.send("launcher-download")', download)
        self.assertNotIn('sessionForLaunch()', download)
        self.assertNotIn('verifyForDownload()', download)
        # Installing public files must not weaken ownership checks for Play.
        launch = swift.split('private func startGame() async {', 1)[1]
        self.assertIn('try await account.sessionForLaunch()', launch)
        native = (ROOT / 'crates/horizon-client/src/game/ios_native_launcher.rs').read_text()
        self.assertIn('Ok(()) if self.native_session.is_some()', native)

    def test_interrupted_download_can_resume_without_redownloading_entire_archive(self):
        swift = VIEW.read_text()
        self.assertIn('cancel(byProducingResumeData:', swift)
        self.assertIn('downloadTask(withResumeData:', swift)
        self.assertIn('NSURLSessionDownloadTaskResumeData', swift)
        self.assertIn('func resumeKey(', swift)
        self.assertIn('func saveResume(', swift)
        self.assertIn('func loadResume(', swift)
        self.assertIn('func clearResume(', swift)
        self.assertIn('didResumeAtOffset', swift)

    def test_cancellation_waits_for_resume_data_before_reenabling_download(self):
        swift = VIEW.read_text()
        self.assertIn('@Published private(set) var pausing = false', swift)
        self.assertIn('self.pausing = false', swift)
        self.assertIn('guard busy && !installing && !pausing', swift)
        self.assertIn('Button("中断中…")', swift)
        self.assertIn('self.busy = false', swift)

    def test_invalid_resume_data_can_recover_with_a_new_download(self):
        swift = VIEW.read_text()
        self.assertIn('activeWasResumed', swift)
        self.assertNotIn('URLError.Code.cannotResume', swift)
        self.assertIn('self.clearResume()', swift)
        self.assertIn('self.endBackground()', swift)

    def test_download_work_keeps_ios_app_alive_long_enough_to_pause_safely(self):
        swift = VIEW.read_text()
        self.assertIn('beginBackgroundTask(', swift)
        self.assertIn('endBackgroundTask(', swift)
        self.assertIn('activeTask', swift)
        self.assertIn('func finishDownload(', swift)
        self.assertIn('SHA256()', swift)
        self.assertIn('Self.limit', swift)
        self.assertIn('current.invalidateAndCancel()', swift)


if __name__ == '__main__':
    unittest.main()
