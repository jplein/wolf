//! Writes Ghostty's `xterm-ghostty` terminfo entry, as terminfo source, to stdout.
//!
//! `ghostty` is the release's `src/terminfo/ghostty.zig`, passed as a named module with
//! `-Mghostty=…`, since Zig refuses an import from outside the root module's directory.
//! The output is what `tic -x -o <dir>` compiles into Wolf's terminfo database.

const std = @import("std");
const ghostty = @import("ghostty");

pub fn main(init: std.process.Init) !void {
    var buffer: [1024]u8 = undefined;
    var stdout_writer = std.Io.File.stdout().writerStreaming(init.io, &buffer);
    try ghostty.ghostty.encode(&stdout_writer.interface);
    try stdout_writer.end();
}
