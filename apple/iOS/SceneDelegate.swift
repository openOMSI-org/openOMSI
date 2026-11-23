import AVFoundation
import SwiftUI
import UIKit

/// The app's scene (UIKit's scene life cycle, which the systems after iOS 26 require): its
/// window shows the launcher. The game's window is put on the same scene, in front, while a
/// drive lasts (GameHost.swift).
@objc(SceneDelegate)
final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?

    func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options connectionOptions: UIScene.ConnectionOptions) {
        guard let scene = scene as? UIWindowScene else { return }
        GameHost.shared.scene = scene
        // the game's sound plays beside other apps' music, and keeps to the silent switch
        try? AVAudioSession.sharedInstance().setCategory(.ambient, mode: .default)
        try? AVAudioSession.sharedInstance().setActive(true)
        let store = GameHost.shared.store
        Theme.registerFonts()
        // (the system's bars in Inter as well)
        let item = UITabBarItem.appearance()
        item.setTitleTextAttributes([.font: Theme.uiFont(10, .medium)], for: .normal)
        UINavigationBar.appearance().titleTextAttributes = [.font: Theme.uiFont(17, .semibold)]
        UIBarButtonItem.appearance().setTitleTextAttributes([.font: Theme.uiFont(17)], for: .normal)
        store.open()
        let window = UIWindow(windowScene: scene)
        let root = LauncherHost(rootView: AnyView(LauncherRoot().environment(store).preferredColorScheme(.dark)))
        root.view.backgroundColor = UIColor(Theme.bg)
        window.rootViewController = root
        window.backgroundColor = UIColor(Theme.bg)
        window.makeKeyAndVisible()
        self.window = window
        GameHost.shared.launcherWindow = window
        // (`OMSI_APPLE_ORIENT=landscape|portrait`: the screen turned so, for looking at either
        // layout on a Simulator without a person to turn it)
        if let o = ProcessInfo.processInfo.environment["OMSI_APPLE_ORIENT"] {
            let mask: UIInterfaceOrientationMask = o == "portrait" ? .portrait : .landscapeRight
            DispatchQueue.main.async {
                scene.requestGeometryUpdate(.iOS(interfaceOrientations: mask)) { e in
                    NSLog("openBus: OMSI_APPLE_ORIENT: \(e)")
                }
            }
        }
        // (the game may put its windows on the scene from now on)
        openbus_ios_scene_ready()
    }

    func sceneDidBecomeActive(_ scene: UIScene) {
        GameHost.shared.store.tick()
    }
}

final class LauncherHost: UIHostingController<AnyView> {
    override var supportedInterfaceOrientations: UIInterfaceOrientationMask {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return .all }
        let store = GameHost.shared.store
        guard store.onboarding else { return .landscape }
        return store.onboardingStep == 2 ? .allButUpsideDown : .portrait
    }
}
