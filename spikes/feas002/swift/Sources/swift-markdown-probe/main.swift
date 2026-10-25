import Foundation
import Markdown

// Deliberately parse and walk, including source-range access, like the C/Rust probes.
@inline(never)
func parseAndWalk(_ input: String) -> (Int, Int) {
    let document = Document(parsing: input, options: [.disableSmartOpts])
    var stack: [any Markup] = [document]
    var nodes = 0
    var positioned = 0
    while let node = stack.popLast() {
        nodes += 1
        if let range = node.range, range.lowerBound.line > 0 {
            positioned += 1
        }
        stack.append(contentsOf: node.children.reversed())
    }
    return (nodes, positioned)
}

let args = CommandLine.arguments
guard args.count == 3, let iterations = Int(args[2]), iterations > 0 else {
    fputs("usage: swift-markdown-probe <file> <iterations>\n", stderr)
    exit(2)
}
let data = try Data(contentsOf: URL(fileURLWithPath: args[1]))
guard let input = String(data: data, encoding: .utf8) else {
    fputs("input is not UTF-8\n", stderr)
    exit(1)
}
var counts = parseAndWalk(input) // Untimed warmup; include destruction per iteration.
let start = DispatchTime.now().uptimeNanoseconds
for _ in 0..<iterations {
    counts = parseAndWalk(input)
}
let elapsed = DispatchTime.now().uptimeNanoseconds - start
print("engine,file,bytes,iterations,elapsed_ns,ns_per_iteration,units,positioned_units")
print("swift-markdown,\(args[1]),\(data.count),\(iterations),\(elapsed),\(elapsed / UInt64(iterations)),\(counts.0),\(counts.1)")
