// Datasheet Mono ring glyphs: a 24-unit grid inside a 32-unit box, square caps, miter joins,
// orange only for "needs you". Usage: draw <out dir> [light]
// Ink is white for a dark cell (<name>.png); `light` draws ink #111214 for a light cell
// (<name>.light.png), which the ring uses in the light appearance and on a pressed cell in dark.
import AppKit

let out = CommandLine.arguments[1]
let light = CommandLine.arguments.count > 2 && CommandLine.arguments[2] == "light"
let white = light
    ? CGColor(srgbRed: 0x11 / 255.0, green: 0x12 / 255.0, blue: 0x14 / 255.0, alpha: 1)
    : CGColor(srgbRed: 1, green: 1, blue: 1, alpha: 1)
let orange = CGColor(srgbRed: 1, green: 0x4F / 255.0, blue: 0x1F / 255.0, alpha: 1)

enum Mark { case line([CGPoint]), fill(CGRect, CGColor) }
func p(_ pts: (CGFloat, CGFloat)...) -> Mark { .line(pts.map { CGPoint(x: $0.0, y: $0.1) }) }

let glyphs: [String: [Mark]] = [
    "Return": [p((19, 4), (19, 14), (6, 14)), p((10, 10), (6, 14), (10, 18))],
    "Go to waiting": [p((3, 8), (3, 3), (8, 3)), p((16, 3), (21, 3), (21, 8)), p((21, 16), (21, 21), (16, 21)),
                      p((8, 21), (3, 21), (3, 16)), .fill(CGRect(x: 8, y: 8, width: 8, height: 8), orange)],
    "New agent": [p((3, 7), (8, 12), (3, 17)), .fill(CGRect(x: 11, y: 15, width: 7, height: 3), white),
                  p((19, 2), (19, 9)), p((15.5, 5.5), (22.5, 5.5))],
]

let pixels = 128
for (name, marks) in glyphs {
    let ctx = CGContext(data: nil, width: pixels, height: pixels, bitsPerComponent: 8, bytesPerRow: 0,
                        space: CGColorSpace(name: CGColorSpace.sRGB)!,
                        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    ctx.translateBy(x: 0, y: CGFloat(pixels))
    ctx.scaleBy(x: CGFloat(pixels) / 32, y: -CGFloat(pixels) / 32)
    ctx.translateBy(x: 4, y: 4)
    ctx.setLineCap(.square)
    ctx.setLineJoin(.miter)
    ctx.setLineWidth(2)
    ctx.setStrokeColor(white)
    for mark in marks {
        switch mark {
        case .line(let pts):
            ctx.addLines(between: pts)
            ctx.strokePath()
        case .fill(let rect, let color):
            ctx.setFillColor(color)
            ctx.fill(rect)
        }
    }
    let png = NSBitmapImageRep(cgImage: ctx.makeImage()!).representation(using: .png, properties: [:])!
    try! png.write(to: URL(fileURLWithPath: out).appendingPathComponent("\(name)\(light ? ".light" : "").png"))
}
