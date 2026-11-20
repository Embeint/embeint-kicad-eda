// SPDX-License-Identifier: GPL-3.0-or-later
//
// Compiles the KiCad IPC protocol (api/proto) into a FileDescriptorSet at build time with protox,
// a pure-Rust protobuf compiler, so no protoc is needed.  The crate works with dynamic messages
// built from these descriptors, the same descriptors the Python MCP's generated code comes from.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use prost::Message;

fn collect(dir: &Path, root: &Path, out: &mut Vec<String>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.expect("directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, root, out);
        } else if path.extension().is_some_and(|ext| ext == "proto") {
            let relative = path.strip_prefix(root).expect("under the proto root");
            let name: Vec<_> = relative.iter().map(|part| part.to_string_lossy()).collect();
            out.push(name.join("/"));
        }
    }
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../../api/proto");
    let root = root
        .canonicalize()
        .unwrap_or_else(|e| panic!("KiCad protocol directory {}: {e}", root.display()));
    println!("cargo:rerun-if-changed={}", root.display());

    let mut files = Vec::new();
    collect(&root, &root, &mut files);
    for file in &files {
        println!("cargo:rerun-if-changed={}", root.join(file).display());
    }

    // Several IPC handlers reply with Empty without importing it in a request
    // declaration. Generic command dispatch must be able to decode those replies.
    files.push("google/protobuf/empty.proto".into());

    let set = protox::compile(&files, [&root]).unwrap_or_else(|e| panic!("protox: {e:?}"));
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("kiapi_descriptors.bin");
    fs::write(&out, set.encode_to_vec()).expect("write the descriptor set");
}
