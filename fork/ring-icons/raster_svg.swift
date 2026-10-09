// Rasterize an SVG to a 128 px PNG with a transparent background. Usage: raster <in.svg> <out.png>
// Used for the MouthKeys grin: `sed 's/#FFFFFF/#111214/g' mouthkeys-grin.svg` gives its light-cell ink.
import AppKit

let args = CommandLine.arguments
guard let image = NSImage(contentsOf: URL(fileURLWithPath: args[1])) else { fatalError("cannot read \(args[1])") }
let pixels = 128
let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels, bitsPerSample: 8,
                           samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                           bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
image.draw(in: NSRect(x: 0, y: 0, width: pixels, height: pixels))
NSGraphicsContext.restoreGraphicsState()
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: args[2]))
