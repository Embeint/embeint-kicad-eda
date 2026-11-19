// SPDX-License-Identifier: GPL-3.0-or-later
//
// Node.js runner for the parity harness: the same contract as the native embeint-layout runner,
// through the WebAssembly build and the browser loader.
//
//   node run_wasm.mjs embeint_web.wasm < requests.jsonl              responses, in order
//   node run_wasm.mjs embeint_web.wasm --bench N < requests.jsonl    {"ns_per_call": ...} lines

import { readFileSync } from "node:fs";
import { loadEmbeintLayout } from "../embeint-web/js/embeint_layout.mjs";

const [wasmPath, flag, count] = process.argv.slice(2);
const iterations = flag === "--bench" ? Number.parseInt(count, 10) : 0;

if (!wasmPath || (flag !== undefined && !(iterations > 0)))
{
    console.error("usage: node run_wasm.mjs embeint_web.wasm [--bench N] < requests.jsonl");
    process.exit(2);
}

const layout = await loadEmbeintLayout(readFileSync(wasmPath));
const lines = readFileSync(0, "utf8").split("\n").filter((line, index, all) => index < all.length - 1 || line);
const out = [];

for (const line of lines)
{
    if (!iterations)
    {
        out.push(layout.layoutBox(line));
        continue;
    }

    let best = Infinity;

    for (let round = 0; round < 5; round++)
    {
        const start = process.hrtime.bigint();

        for (let i = 0; i < iterations; i++)
            layout.layoutBox(line);

        best = Math.min(best, Number(process.hrtime.bigint() - start) / iterations);
    }

    out.push(JSON.stringify({ ns_per_call: Math.round(best * 10) / 10 }));
}

process.stdout.write(out.length ? out.join("\n") + "\n" : "");
