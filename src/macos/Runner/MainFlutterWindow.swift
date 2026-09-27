import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    // Match the Flutter-side geometry constraints and keep the window usable
    // on smaller MacBook displays.
    self.minSize = NSSize(width: 800, height: 600)

    // Native macOS chrome: keep the traffic lights while allowing Flutter
    // content to extend into a transparent title-bar area.
    self.titleVisibility = .hidden
    self.titlebarAppearsTransparent = true
    self.styleMask.insert(.fullSizeContentView)
    self.tabbingMode = .disallowed
    self.isReleasedWhenClosed = false

    if #available(macOS 11.0, *) {
      self.titlebarSeparatorStyle = .none
    }

    RegisterGeneratedPlugins(registry: flutterViewController)

    super.awakeFromNib()
  }
}
