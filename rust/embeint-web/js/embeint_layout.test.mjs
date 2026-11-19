// SPDX-License-Identifier: GPL-3.0-or-later
//
// Loader tests that need no Rust build: a hand-assembled module with the embeint-web exports
// stands in for the real one.  The real module is exercised by rust/parity (run_wasm.mjs).
//
//   node --test rust/embeint-web/js/embeint_layout.test.mjs

import assert from "node:assert/strict";
import test from "node:test";
import { EmbeintLayoutBrokenError, loadEmbeintLayout } from "./embeint_layout.mjs";

const RESPONSE = '{"ok":1}';

function leb(value)
{
    const out = [];

    do
    {
        let byte = value & 0x7f;
        value >>>= 7;
        out.push(value ? byte | 0x80 : byte);
    } while (value);

    return out;
}

const vector = (items) => [...leb(items.length), ...items.flat()];
const section = (id, bytes) => [id, ...leb(bytes.length), ...bytes];
const name = (text) => vector([...new TextEncoder().encode(text)]);

/**
 * A module exporting memory, embeint_alloc, embeint_free and embeint_layout_box.  The memory
 * holds RESPONSE at address 32.  embeint_layout_box returns it packed when the request length is
 * not 1, and executes `unreachable` (a trap) when it is.
 */
function fakeModule()
{
    const I32 = 0x7f, I64 = 0x7e;
    const response = [...new TextEncoder().encode(RESPONSE)];
    const types = vector([
        [0x60, 1, I32, 1, I32],          // (i32) -> i32
        [0x60, 2, I32, I32, 0],          // (i32, i32) -> ()
        [0x60, 2, I32, I32, 1, I64],     // (i32, i32) -> i64
    ]);
    const functions = vector([[0], [1], [2]]);
    const memory = vector([[0, 1]]);
    const exports = vector([
        [...name("memory"), 2, 0],
        [...name("embeint_alloc"), 0, 0],
        [...name("embeint_free"), 0, 1],
        [...name("embeint_layout_box"), 0, 2],
    ]);
    const body = (code) => vector([0, ...code, 0x0b]);  // no locals
    const code = vector([
        body([0x41, 16]),                                        // i32.const 16
        body([]),                                                // nothing to free
        body([0x20, 1, 0x41, 1, 0x46, 0x04, 0x40, 0x00, 0x0b,    // if len == 1: unreachable
              0x42, ...sleb64(32n << 32n | BigInt(response.length))]),
    ]);
    const data = vector([[0, 0x41, 32, 0x0b, ...vector(response)]]);

    return new Uint8Array([
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
        ...section(1, types), ...section(3, functions), ...section(5, memory),
        ...section(7, exports), ...section(10, code), ...section(11, data),
    ]);
}

function sleb64(value)
{
    const out = [];

    for (;;)
    {
        const byte = Number(value & 0x7fn);
        value >>= 7n;

        if ((value === 0n && !(byte & 0x40)) || (value === -1n && byte & 0x40))
            return [...out, byte];

        out.push(byte | 0x80);
    }
}

test("a call returns the response", async () =>
{
    const bytes = fakeModule();
    const layout = await loadEmbeintLayout(bytes);

    assert.equal(layout.layoutBox("{}"), RESPONSE);
    assert.equal(layout.layoutBox("[]"), RESPONSE);
    assert.equal(layout.broken, false);
    // A compiled module works too: the cheap way to replace a broken instance.
    assert.equal((await loadEmbeintLayout(await WebAssembly.compile(bytes))).layoutBox("{}"), RESPONSE);
});

test("a trap marks the instance broken and later calls refuse to enter it", async () =>
{
    const layout = await loadEmbeintLayout(fakeModule());

    assert.throws(() => layout.layoutBox("x"), WebAssembly.RuntimeError);
    assert.equal(layout.broken, true);

    for (const request of ["x", "{}"])
    {
        assert.throws(() => layout.layoutBox(request), (error) =>
        {
            assert.ok(error instanceof EmbeintLayoutBrokenError);
            assert.ok(error.cause instanceof WebAssembly.RuntimeError);
            assert.match(error.message, /trapped earlier/);
            return true;
        });
    }

    // A new instance from the same bytes works.
    assert.equal((await loadEmbeintLayout(fakeModule())).layoutBox("{}"), RESPONSE);
});
