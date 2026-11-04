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
        self.assertIn('if !hasConfiguredPublisher { showPersonalLink = true }', swift)
        self.assertIn('if hasConfiguredPublisher {', swift)

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
        self.assertIn('URLError.Code.cannotResume.rawValue', swift)
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
