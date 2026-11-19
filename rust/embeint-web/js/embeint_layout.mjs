// SPDX-License-Identifier: GPL-3.0-or-later
//
// Load the embeint-web WebAssembly module and call it with JSON text.  Works unchanged in a
// browser, a Web Worker (preferred for long layouts) and Node.js: it uses only WebAssembly,
// TextEncoder and TextDecoder.
//
//   const layout = await loadEmbeintLayout(await (await fetch("embeint_web.wasm")).arrayBuffer());
//   const response = JSON.parse(layout.layoutBox(JSON.stringify({ pins: [...] })));
//
// A trap (WebAssembly.RuntimeError, or a stack overflow) inside the module leaves its memory in
// an unknown state, so the instance is never used again.  The call that trapped rethrows the
// original error; every later call throws an EmbeintLayoutBrokenError (with the trap as its
// `cause`) without entering the module, and `layout.broken` becomes true.  The loader does not
// re-instantiate by itself: synchronous instantiation is restricted on a browser's main thread,
// so call loadEmbeintLayout again (it accepts a compiled WebAssembly.Module as well as bytes).
// Request errors are not traps: they come back as {"error": ...} JSON.

/** Thrown by layoutBox once the module instance has trapped and can no longer be used. */
export class EmbeintLayoutBrokenError extends Error
{
    constructor(cause)
    {
        super("embeint-web: the module instance trapped earlier and cannot be used again; "
              + "call loadEmbeintLayout for a new one", { cause });
        this.name = "EmbeintLayoutBrokenError";
    }
}

export async function loadEmbeintLayout(wasmBytesOrModule)
{
    const result = await WebAssembly.instantiate(wasmBytesOrModule, {});
    // instantiate(bytes) resolves to { module, instance }; instantiate(module) to the instance.
    const instance = result instanceof WebAssembly.Instance ? result : result.instance;
    const { memory, embeint_alloc, embeint_free, embeint_layout_box } = instance.exports;
    const encoder = new TextEncoder();
    const decoder = new TextDecoder("utf-8", { fatal: true });
    let trap = null;

    /** Call an export; any exception thrown from inside the module marks the instance broken. */
    function enter(exported, ...args)
    {
        if (trap !== null)
            throw new EmbeintLayoutBrokenError(trap);

        try
        {
            return exported(...args);
        }
        catch (error)
        {
            trap = error;
            throw error;
        }
    }

    /** Lay out a symbol: request JSON text in, response JSON text out (see embeint-layout). */
    function layoutBox(requestText)
    {
        const request = encoder.encode(requestText);
        const input = enter(embeint_alloc, request.length);

        if (input === 0)
            throw new Error("embeint-web: out of memory for the request");

        // The module consumes the request buffer.  Read memory.buffer after each call: an
        // allocation can grow the memory and detach the previous ArrayBuffer.
        new Uint8Array(memory.buffer, input, request.length).set(request);
        const packed = BigInt.asUintN(64, enter(embeint_layout_box, input, request.length));
        const pointer = Number(packed >> 32n);
        const length = Number(packed & 0xffffffffn);

        try
        {
            return decoder.decode(new Uint8Array(memory.buffer, pointer, length));
        }
        finally
        {
            enter(embeint_free, pointer, length);
        }
    }

    return {
        layoutBox,
        /** True once the instance has trapped; layoutBox then always throws. */
        get broken() { return trap !== null; },
    };
}
