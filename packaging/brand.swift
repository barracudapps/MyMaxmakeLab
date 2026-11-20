// Promotional images of the README, composed from real screenshots in
// the colours of the icon. Run:
//   swift packaging/brand.swift <images dir> <output dir>
// The images dir holds: icon.png, design.png, prepare.png, run.png,
// fig.png, mold.png, dome.png, makers.png, ornaments.png.
import AppKit

let input = CommandLine.arguments[1]
let output = CommandLine.arguments[2]
let indigo = NSColor(red: 0.42, green: 0.39, blue: 1.0, alpha: 1)
let coral = NSColor(red: 0.98, green: 0.55, blue: 0.32, alpha: 1)
let ink = NSColor(red: 0.08, green: 0.09, blue: 0.11, alpha: 1)

func load(_ name: String) -> NSImage { NSImage(contentsOfFile: "\(input)/\(name)")! }

func render(_ width: CGFloat, _ height: CGFloat, _ name: String, _ draw: () -> Void) {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: Int(width), pixelsHigh: Int(height),
                               bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    draw()
    NSGraphicsContext.restoreGraphicsState()
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: "\(output)/\(name)"))
}

/// Dark background with a soft glow of the brand colours and a faint
/// bed grid.
func backdrop(_ rect: NSRect, glow: Bool = true) {
    ink.setFill()
    rect.fill()
    if glow {
        let left = NSGradient(colors: [indigo.withAlphaComponent(0.55), indigo.withAlphaComponent(0)])!
        left.draw(fromCenter: NSPoint(x: rect.width * 0.15, y: rect.height * 0.9), radius: 0,
                  toCenter: NSPoint(x: rect.width * 0.15, y: rect.height * 0.9), radius: rect.width * 0.55, options: [])
        let right = NSGradient(colors: [coral.withAlphaComponent(0.45), coral.withAlphaComponent(0)])!
        right.draw(fromCenter: NSPoint(x: rect.width * 0.9, y: rect.height * 0.1), radius: 0,
                   toCenter: NSPoint(x: rect.width * 0.9, y: rect.height * 0.1), radius: rect.width * 0.5, options: [])
    }
    NSColor(white: 1, alpha: 0.04).setStroke()
    let grid = NSBezierPath()
    var x: CGFloat = 0
    while x < rect.width { grid.move(to: NSPoint(x: x, y: 0)); grid.line(to: NSPoint(x: x, y: rect.height)); x += 40 }
    var y: CGFloat = 0
    while y < rect.height { grid.move(to: NSPoint(x: 0, y: y)); grid.line(to: NSPoint(x: rect.width, y: y)); y += 40 }
    grid.lineWidth = 1
    grid.stroke()
}

func text(_ string: String, _ point: NSPoint, size: CGFloat, weight: NSFont.Weight = .regular,
          color: NSColor = .white, width: CGFloat = 2000, centred: Bool = false) {
    let style = NSMutableParagraphStyle()
    style.lineHeightMultiple = 1.05
    style.alignment = centred ? .center : .left
    let attributes: [NSAttributedString.Key: Any] = [
        .font: NSFont.systemFont(ofSize: size, weight: weight), .foregroundColor: color, .paragraphStyle: style,
    ]
    let string = NSAttributedString(string: string, attributes: attributes)
    let bounds = string.boundingRect(with: NSSize(width: width, height: 2000), options: [.usesLineFragmentOrigin])
    string.draw(with: NSRect(x: point.x, y: point.y - bounds.height, width: width, height: bounds.height),
                options: [.usesLineFragmentOrigin])
}

/// Gradient text for the brand name.
func brandName(_ string: String, _ point: NSPoint, size: CGFloat) {
    let font = NSFont.systemFont(ofSize: size, weight: .heavy)
    let attributed = NSAttributedString(string: string, attributes: [.font: font, .foregroundColor: NSColor.white])
    let bounds = attributed.size()
    let mask = NSImage(size: bounds, flipped: false) { _ in attributed.draw(at: .zero); return true }
    let image = NSImage(size: bounds, flipped: false) { rect in
        NSGradient(starting: NSColor.white, ending: NSColor(red: 1, green: 0.8, blue: 0.68, alpha: 1))!.draw(in: rect, angle: 0)
        mask.draw(in: rect, from: .zero, operation: .destinationIn, fraction: 1)
        return true
    }
    image.draw(in: NSRect(x: point.x, y: point.y - bounds.height, width: bounds.width, height: bounds.height))
}

/// Screenshot in a rounded card with a shadow.
func card(_ image: NSImage, _ rect: NSRect, radius: CGFloat = 18, border: Bool = true) {
    NSGraphicsContext.saveGraphicsState()
    let shadow = NSShadow()
    shadow.shadowColor = NSColor.black.withAlphaComponent(0.55)
    shadow.shadowBlurRadius = 40
    shadow.shadowOffset = NSSize(width: 0, height: -14)
    shadow.set()
    NSColor.black.setFill()
    NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius).fill()
    NSGraphicsContext.restoreGraphicsState()
    NSGraphicsContext.saveGraphicsState()
    NSBezierPath(roundedRect: rect, xRadius: radius, yRadius: radius).addClip()
    // Fill the card, cropping the picture to its proportions.
    let size = image.size
    let scale = max(rect.width / size.width, rect.height / size.height)
    let drawn = NSSize(width: size.width * scale, height: size.height * scale)
    image.draw(in: NSRect(x: rect.midX - drawn.width / 2, y: rect.midY - drawn.height / 2, width: drawn.width, height: drawn.height))
    NSGraphicsContext.restoreGraphicsState()
    if border {
        NSColor(white: 1, alpha: 0.12).setStroke()
        let outline = NSBezierPath(roundedRect: rect.insetBy(dx: 0.5, dy: 0.5), xRadius: radius, yRadius: radius)
        outline.lineWidth = 1
        outline.stroke()
    }
}

func chip(_ label: String, _ point: NSPoint, color: NSColor) -> CGFloat {
    let font = NSFont.systemFont(ofSize: 22, weight: .semibold)
    let width = NSAttributedString(string: label, attributes: [.font: font]).size().width + 36
    let rect = NSRect(x: point.x, y: point.y - 44, width: width, height: 44)
    color.withAlphaComponent(0.18).setFill()
    NSBezierPath(roundedRect: rect, xRadius: 22, yRadius: 22).fill()
    color.setStroke()
    let outline = NSBezierPath(roundedRect: rect.insetBy(dx: 1, dy: 1), xRadius: 21, yRadius: 21)
    outline.lineWidth = 1.5
    outline.stroke()
    text(label, NSPoint(x: rect.minX + 18, y: rect.maxY - 9), size: 22, weight: .semibold, color: .white)
    return width
}

// 1. Banner.
render(1800, 560, "banner.png") {
    let rect = NSRect(x: 0, y: 0, width: 1800, height: 560)
    backdrop(rect)
    load("icon.png").draw(in: NSRect(x: 80, y: 190, width: 180, height: 180))
    brandName("MyMaxmakeLab", NSPoint(x: 290, y: 395), size: 92)
    text("Laser & CNC, simple and safe.", NSPoint(x: 296, y: 285), size: 38, weight: .medium,
         color: NSColor(white: 1, alpha: 0.85))
    var x: CGFloat = 296
    for (label, color) in [("Laser", coral), ("CNC 3 · 4 · 5 axes", indigo), ("Safety first", NSColor.systemGreen)] {
        x += chip(label, NSPoint(x: x, y: 215), color: color) + 14
    }
    card(load("prepare.png"), NSRect(x: 1080, y: 80, width: 618, height: 400))
}

// 2. Hero.
render(1800, 1060, "hero.png") {
    let rect = NSRect(x: 0, y: 0, width: 1800, height: 1060)
    backdrop(rect)
    text("From an idea to a finished piece", NSPoint(x: 0, y: 1000), size: 64, weight: .bold, width: 1800, centred: true)
    text("Design, prepare and run laser and CNC jobs on GRBL machines, with a live safety monitor.",
         NSPoint(x: 0, y: 905), size: 28, color: NSColor(white: 1, alpha: 0.75), width: 1800, centred: true)
    // Same proportions as the screenshot so nothing is cropped.
    card(load("prepare.png"), NSRect(x: 290, y: 50, width: 1220, height: 790))
    card(load("fig.png"), NSRect(x: 60, y: 120, width: 330, height: 260))
    card(load("ornaments.png"), NSRect(x: 1430, y: 470, width: 320, height: 260))
}

// 3. Feature tiles.
func tile(_ name: String, title: String, subtitle: String, image: NSImage, accent: NSColor, light: Bool = false) {
    render(900, 600, name) {
        let rect = NSRect(x: 0, y: 0, width: 900, height: 600)
        backdrop(rect, glow: false)
        let glow = NSGradient(colors: [accent.withAlphaComponent(0.5), accent.withAlphaComponent(0)])!
        glow.draw(fromCenter: NSPoint(x: 120, y: 560), radius: 0, toCenter: NSPoint(x: 120, y: 560), radius: 700, options: [])
        accent.setFill()
        NSBezierPath(roundedRect: NSRect(x: 56, y: 520, width: 54, height: 6), xRadius: 3, yRadius: 3).fill()
        text(title, NSPoint(x: 56, y: 505), size: 44, weight: .bold)
        text(subtitle, NSPoint(x: 56, y: 440), size: 22, color: NSColor(white: 1, alpha: 0.75), width: 790)
        if light {
            NSColor.white.setFill()
        }
        card(image, NSRect(x: 56, y: 40, width: 788, height: 330), radius: 16)
    }
}
tile("safety.png", title: "Safe by design",
     subtitle: "Live monitor, limits checked on the real position, approved frame, probed origin, one-click stop.",
     image: load("run.png"), accent: NSColor.systemGreen)
tile("make.png", title: "Make anything",
     subtitle: "Boxes for an object, mazes, puzzles, ornaments by style, sprues with automatic bridges.",
     image: load("makers.png"), accent: coral)
tile("axes.png", title: "3D, 4 and 5 axes",
     subtitle: "Reliefs, molds, whole parts around the A axis, positional 5-axis, true-scale 3D preview.",
     image: load("axes_collage.png"), accent: indigo)
// A conversation with an assistant using the MCP tools.
render(1576, 660, "ai-chat.png") {
    NSColor(red: 0.11, green: 0.12, blue: 0.15, alpha: 1).setFill()
    NSRect(x: 0, y: 0, width: 1576, height: 660).fill()
    let mono = { (string: String, y: CGFloat, color: NSColor, weight: NSFont.Weight) in
        let font = NSFont.monospacedSystemFont(ofSize: 30, weight: weight)
        NSAttributedString(string: string, attributes: [.font: font, .foregroundColor: color])
            .draw(at: NSPoint(x: 48, y: y))
    }
    mono("> Make a box for a 70 x 45 x 30 mm Raspberry Pi", 560, .white, .semibold)
    mono("  in 3 mm plywood, then the laser G-code.", 515, .white, .semibold)
    mono("  generate_box      pi-box.svg   6 panels, kerf 0.1", 420, NSColor.systemGreen, .regular)
    mono("  svg_to_gcode      pi-box.nc    2 min 41 s, 1 018 lines", 370, NSColor.systemGreen, .regular)
    mono("  analyze_gcode     82 x 58 mm, inside the bed", 320, NSColor.systemGreen, .regular)
    mono("Open pi-box.nc in MyMaxmakeLab, frame it, start.", 210, NSColor(white: 1, alpha: 0.7), .regular)
    mono("The server never moves the machine.", 160, coral, .medium)
}
tile("ai.png", title: "AI-ready",
     subtitle: "An MCP server lets Claude, Codex or Gemini design and prepare jobs. It never moves the machine.",
     image: NSImage(contentsOfFile: "\(output)/ai-chat.png")!, accent: NSColor.systemPink)
try? FileManager.default.removeItem(atPath: "\(output)/ai-chat.png")
