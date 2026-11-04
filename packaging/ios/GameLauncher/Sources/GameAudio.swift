import AVFoundation

/// The Rust game's CoreAudio output needs an active iOS playback session.
/// Keep the session inactive while the native launcher is on screen.
/// Adapted from the MIT-licensed heyskylo/horizon-ios native launcher.
@MainActor
enum GameAudio {
    private static var interruptionObserver: NSObjectProtocol?

    static func start() {
        let session = AVAudioSession.sharedInstance()
        do {
            // A game should be audible with the ring/silent switch on, without
            // interrupting another app's music or an already-running call.
            try session.setCategory(.playback, mode: .default, options: [.mixWithOthers])
            try session.setActive(true)
        } catch {
            NSLog("Horizon: unable to activate iOS game audio: %@", error.localizedDescription)
        }
        guard interruptionObserver == nil else { return }
        interruptionObserver = NotificationCenter.default.addObserver(
            forName: AVAudioSession.interruptionNotification,
            object: session,
            queue: .main
        ) { notification in
            let raw = notification.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt
            guard raw.flatMap(AVAudioSession.InterruptionType.init(rawValue:)) == .ended else { return }
            // A phone call or alarm may take over the session. Restore game
            // audio when the interruption ends, never during the call.
            Task { @MainActor in
                guard interruptionObserver != nil else { return }
                do { try AVAudioSession.sharedInstance().setActive(true) }
                catch { NSLog("Horizon: unable to resume game audio: %@", error.localizedDescription) }
            }
        }
    }

    static func stop() {
        if let observer = interruptionObserver {
            NotificationCenter.default.removeObserver(observer)
            interruptionObserver = nil
        }
        do {
            try AVAudioSession.sharedInstance().setActive(
                false, options: .notifyOthersOnDeactivation
            )
        } catch {
            NSLog("Horizon: unable to deactivate game audio: %@", error.localizedDescription)
        }
    }
}
