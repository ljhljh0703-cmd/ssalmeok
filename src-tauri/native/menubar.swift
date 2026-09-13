import AppKit
import Foundation

private struct TrayPayload: Decodable {
    let icon: String
    let title: String
    let tooltip: String
    let codexLines: [String]
    let claudeLines: [String]
    let costNote: String
}

private final class TrayTarget: NSView {
    weak var statusItem: NSStatusItem?

    private func setHighlighted(_ highlighted: Bool) {
        (superview as? NSStatusBarButton)?.highlight(highlighted)
    }

    private func emit(_ action: String) {
        guard let data = "\(action)\n".data(using: .utf8) else { return }
        do {
            try FileHandle.standardOutput.write(contentsOf: data)
        } catch {
            logError("메뉴 동작 전송 실패: \(error.localizedDescription)")
        }
    }

    override func mouseDown(with event: NSEvent) {
        setHighlighted(true)
    }

    override func mouseUp(with event: NSEvent) {
        setHighlighted(false)
        emit("toggle")
    }

    override func rightMouseDown(with event: NSEvent) {
        setHighlighted(true)
    }

    override func rightMouseUp(with event: NSEvent) {
        setHighlighted(false)
        if let contextMenu = menu, let statusItem {
            statusItem.menu = contextMenu
            statusItem.button?.performClick(nil)
            statusItem.menu = nil
        }
    }

    @objc func openApp(_ sender: Any?) {
        emit("open")
    }

    @objc func refreshUsage(_ sender: Any?) {
        emit("refresh")
    }

    @objc func quitApp(_ sender: Any?) {
        emit("quit")
    }
}

private final class MenubarController: NSObject, NSApplicationDelegate {
    private let codexIcon: NSImage?
    private let claudeIcon: NSImage?
    private let statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
    private let target = TrayTarget(frame: .zero)

    init(codexIconPath: String, claudeIconPath: String) {
        codexIcon = MenubarController.loadIcon(at: codexIconPath)
        claudeIcon = MenubarController.loadIcon(at: claudeIconPath)
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApplication.shared.setActivationPolicy(.accessory)
        statusItem.isVisible = true

        guard let button = statusItem.button else {
            logError("macOS 메뉴 막대 버튼을 만들지 못했습니다.")
            NSApplication.shared.terminate(nil)
            return
        }

        button.imagePosition = .imageLeft
        target.statusItem = statusItem
        target.frame = button.bounds
        target.autoresizingMask = [.width, .height]
        button.addSubview(target)

        apply(
            TrayPayload(
                icon: "codex",
                title: "…",
                tooltip: "Codex · Claude 남은 사용량을 읽는 중",
                codexLines: ["Codex  ·  읽는 중"],
                claudeLines: ["Claude  ·  읽는 중"],
                costNote: "※ 실제 청구액이 아닌 개발자용 정가 환산"
            )
        )
        readUpdates()
    }

    private static func loadIcon(at path: String) -> NSImage? {
        guard let image = NSImage(contentsOfFile: path) else {
            logError("서비스 아이콘을 읽지 못했습니다: \(path)")
            return nil
        }
        image.size = NSSize(width: 18, height: 18)
        image.isTemplate = false
        return image
    }

    private func readUpdates() {
        DispatchQueue.global(qos: .utility).async { [weak self] in
            while let line = readLine(strippingNewline: true) {
                guard let data = line.data(using: .utf8) else { continue }
                do {
                    let payload = try JSONDecoder().decode(TrayPayload.self, from: data)
                    DispatchQueue.main.async {
                        self?.apply(payload)
                    }
                } catch {
                    logError("메뉴 상태 해석 실패: \(error.localizedDescription)")
                }
            }
            DispatchQueue.main.async {
                NSApplication.shared.terminate(nil)
            }
        }
    }

    private func apply(_ payload: TrayPayload) {
        guard let button = statusItem.button else { return }
        button.image = payload.icon == "claude" ? claudeIcon : codexIcon
        button.title = payload.title
        button.toolTip = payload.tooltip
        target.frame = button.bounds
        target.menu = buildMenu(payload)
    }

    private func buildMenu(_ payload: TrayPayload) -> NSMenu {
        let menu = NSMenu()
        menu.autoenablesItems = false

        addInfoLines(payload.codexLines, to: menu)
        menu.addItem(.separator())
        addInfoLines(payload.claudeLines, to: menu)
        addInfo(payload.costNote, to: menu, indented: false)
        menu.addItem(.separator())
        addAction("쌀먹 열기", selector: #selector(TrayTarget.openApp(_:)), to: menu)
        addAction("지금 갱신", selector: #selector(TrayTarget.refreshUsage(_:)), to: menu)
        menu.addItem(.separator())
        addAction("종료", selector: #selector(TrayTarget.quitApp(_:)), to: menu)
        return menu
    }

    private func addInfoLines(_ lines: [String], to menu: NSMenu) {
        for (index, line) in lines.enumerated() {
            addInfo(line, to: menu, indented: index > 0)
        }
    }

    private func addInfo(_ title: String, to menu: NSMenu, indented: Bool) {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.isEnabled = false
        item.indentationLevel = indented ? 1 : 0
        menu.addItem(item)
    }

    private func addAction(_ title: String, selector: Selector, to menu: NSMenu) {
        let item = NSMenuItem(title: title, action: selector, keyEquivalent: "")
        item.target = target
        item.isEnabled = true
        menu.addItem(item)
    }
}

private func logError(_ message: String) {
    guard let data = "\(message)\n".data(using: .utf8) else { return }
    try? FileHandle.standardError.write(contentsOf: data)
}

@main
private struct MenubarMain {
    static func main() {
        let arguments = CommandLine.arguments
        guard arguments.count == 3 else {
            logError("사용법: ssalmeok-menubar <codex-icon> <claude-icon>")
            exit(2)
        }

        let application = NSApplication.shared
        let mainMenu = NSMenu()
        let appMenuItem = NSMenuItem()
        appMenuItem.submenu = NSMenu(title: "쌀먹")
        mainMenu.addItem(appMenuItem)
        application.mainMenu = mainMenu
        let controller = MenubarController(
            codexIconPath: arguments[1],
            claudeIconPath: arguments[2]
        )
        application.delegate = controller
        withExtendedLifetime(controller) {
            application.run()
        }
    }
}
