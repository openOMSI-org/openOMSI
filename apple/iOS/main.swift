// openBus on an iPhone or an iPad: the game library runs the app (winit's event loop is
// UIKit's `UIApplicationMain`); this app's scene delegate shows the launcher on the scene
// (see SceneDelegate.swift) and the game's window joins it for a drive (GameHost.swift).
import Foundation

// (an OMSI 2 folder outside the app chosen before: its access first, the game looks for it
// as it starts)
Platform.restoreAccess()
// (the launcher in the language of the game's settings: HOME is the app's container here, the
// game keeps its data in Library/Application Support/openBus, see ios.rs)
AppLanguage.apply(home: URL(fileURLWithPath: NSHomeDirectory()).appendingPathComponent("Library/Application Support/openBus"))
openbus_ios_main()
