// SPDX-License-Identifier: Apache-2.0

import Foundation
import ProvekitLiftSwiftSource

if CommandLine.arguments.contains("--rpc") {
    SwiftSourceRPC.run()
} else {
    FileHandle.standardError.write("usage: provekit-lift-swift-source --rpc\n".data(using: .utf8)!)
    exit(1)
}
