// Draws the application icon (1024 px PNG): a warm rounded tile, a
// work piece and a laser beam. Run: swift packaging/make-icon.swift out.png
import AppKit

let size = 1024.0
let image = NSImage(size: NSSize(width: size, height: size))
image.lockFocus()
let tile = NSBezierPath(roundedRect: NSRect(x: 64, y: 64, width: 896, height: 896), xRadius: 200, yRadius: 200)
NSGradient(starting: NSColor(red: 0.42, green: 0.39, blue: 1.0, alpha: 1),
           ending: NSColor(red: 0.98, green: 0.55, blue: 0.32, alpha: 1))!.draw(in: tile, angle: -60)
// Work piece.
NSColor(white: 1, alpha: 0.92).setFill()
NSBezierPath(roundedRect: NSRect(x: 220, y: 230, width: 584, height: 150), xRadius: 30, yRadius: 30).fill()
// Engraved wave on the piece.
let wave = NSBezierPath()
wave.move(to: NSPoint(x: 270, y: 305))
for step in 0...40 {
    let x = 270.0 + Double(step) * 12.0
    wave.line(to: NSPoint(x: x, y: 305 + 35 * sin(Double(step) / 40 * 2 * .pi * 2)))
}
NSColor(red: 0.36, green: 0.22, blue: 0.12, alpha: 1).setStroke()
wave.lineWidth = 22
wave.lineCapStyle = .round
wave.stroke()
// Head and beam.
NSColor(white: 1, alpha: 1).setFill()
NSBezierPath(roundedRect: NSRect(x: 600, y: 610, width: 170, height: 200), xRadius: 36, yRadius: 36).fill()
let beam = NSBezierPath()
beam.move(to: NSPoint(x: 685, y: 610))
beam.line(to: NSPoint(x: 685, y: 400))
NSColor(red: 1, green: 0.25, blue: 0.2, alpha: 1).setStroke()
beam.lineWidth = 26
beam.lineCapStyle = .round
beam.stroke()
NSColor(red: 1, green: 0.85, blue: 0.3, alpha: 1).setFill()
NSBezierPath(ovalIn: NSRect(x: 650, y: 360, width: 70, height: 70)).fill()
image.unlockFocus()
let bitmap = NSBitmapImageRep(data: image.tiffRepresentation!)!
try! bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
