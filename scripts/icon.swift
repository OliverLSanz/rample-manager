import AppKit
import Foundation

let output = CommandLine.arguments[1]
let size = 1024
let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
let body = NSBezierPath(roundedRect: NSRect(x: 48, y: 48, width: 928, height: 928), xRadius: 212, yRadius: 212)
NSColor(calibratedRed: 0.10, green: 0.12, blue: 0.14, alpha: 1).setFill(); body.fill()
NSColor.white.withAlphaComponent(0.09).setStroke(); body.lineWidth = 5; body.stroke()
let colors = [NSColor(calibratedRed: 0.96, green: 0.49, blue: 0.36, alpha: 1), NSColor(calibratedRed: 0.51, green: 0.79, blue: 0.68, alpha: 1), NSColor(calibratedRed: 0.67, green: 0.61, blue: 0.87, alpha: 1), NSColor(calibratedRed: 0.87, green: 0.76, blue: 0.46, alpha: 1)]
for row in 0..<4 {
    for col in 0..<4 {
        let filled = [[true, false, true, false], [false, true, false, true], [true, true, true, true], [false, false, true, false]][row][col]
        (filled ? colors[row] : NSColor.white.withAlphaComponent(0.07)).setFill()
        NSBezierPath(roundedRect: NSRect(x: 208 + col * 157, y: 658 - row * 145, width: 133, height: 115), xRadius: 23, yRadius: 23).fill()
    }
}
NSGraphicsContext.restoreGraphicsState()
try bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: output))
