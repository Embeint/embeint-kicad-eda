// SPDX-License-Identifier: GPL-3.0-or-later
//
// Load the embeint-web WebAssembly module and call it with JSON text.  Works unchanged in a
// browser, a Web Worker (preferred for long layouts) and Node.js: it uses only WebAssembly,
// TextEncoder and TextDecoder.
//
//   const layout = await loadEmbeintLayout(await (await fetch("embeint_web.wasm")).arrayBuffer());
//   const response = JSON.parse(layout.layoutBox(JSON.stringify({ pins: [...] })));

export async function loadEmbeintLayout(wasmBytes)
{
    const { instance } = await WebAssembly.instantiate(wasmBytes, {});
    const { memory, embeint_alloc, embeint_free, embeint_layout_box } = instance.exports;
    const encoder = new TextEncoder();
    const decoder = new TextDecoder("utf-8", { fatal: true });

    /** Lay out a symbol: request JSON text in, response JSON text out (see embeint-layout). */
    function layoutBox(requestText)
    {
        const request = encoder.encode(requestText);
        const input = embeint_alloc(request.length);

        if (input === 0)
            throw new Error("embeint-web: out of memory for the request");

        // The module consumes the request buffer.  Read memory.buffer after each call: an
        // allocation can grow the memory and detach the previous ArrayBuffer.
        new Uint8Array(memory.buffer, input, request.length).set(request);
        const packed = BigInt.asUintN(64, embeint_layout_box(input, request.length));

        if (packed === 0n)
            throw new Error("embeint-web: out of memory for the response");

        const pointer = Number(packed >> 32n);
        const length = Number(packed & 0xffffffffn);

        try
        {
            return decoder.decode(new Uint8Array(memory.buffer, pointer, length));
        }
        finally
        {
            embeint_free(pointer, length);
        }
    }

    return { layoutBox };
}
