def check_inventory(elock, glock, phase):
    """Every check name a fully passing verify run produces for these locks (R3 handoff)."""
    names = ['eligibility lock: top-level fields', 'eligibility lock: canonical semantic fields',
             'graphs lock: top-level fields']
    names += [f'eligibility lock: {k}' for k in lockschema.ELIGIBILITY[1]]
    names += [f'graphs lock: {k}' for k in lockschema.GRAPHS[1]]
    L = elock
    names += ['lock status is experimental, not qualified', 'oracle sha256', 'oracle build id', 'ELF64 LSB x86-64 executable',
              'x86 ISA needed: baseline only', 'ABI tag', '.comment names the reviewed compiler', 'compile_commands sha256',
              f'all {L["compile_count"]} compilations strict, no forbidden FP/ISA flags',
              'adapter compiled once with reviewed command', 'header include dir present in adapter command',
              'retained compiler-input tree provided (--oracle-src)',
              'no earlier include dir shadows math/matrix3x3.h, vector2d.h, vector3.h',
              'retained compiler-input files equal the bound files',
              'no MXCSR/x87-control writes anywhere in .text (objdump scan)', 'candidate image identity (docker image inspect)',
              'oracle provenance sha256', 'compiler identity and target', 'provenance FP mode: contract off, no fast-math',
              'provenance pins', 'provenance libc', 'source manifest sha256', 'source manifest revision/tree/pin/clean',
              'manifest adapter hash equals compiler input', 'provenance overlay adapter hash equals compiler input',
              'adapter definitions equal the reviewed definitions', 'no FP-environment functions referenced',
              'runtime FP-mode contract recorded (measurement is runtime qualification)']
    names += [f'{k} is Git blob {b["blob"][:12]} ({b["git_path"]})' for k, b in L['source']['blobs'].items()]
    names += [f'reviewed callee {a} {b["effect"]} bytes' for a, b in L['callees'].items()]
    f = L['frame']
    names += ['r12/r15 prologue bindings (straight-line, sources unwritten)', 'aOp -> %rbx prologue binding (mov %rsi,%rbx)',
              f'frame setup {f["setup_text"]!r} and reviewed frame size {f["size"]:#x}', CONTEXT_IDENTITY, CONTEXT_CITATIONS,
              CONTEXT_RECORD]
    for sid in contract.REQUIRED_SITES:
        names += [f'{sid}: r12 (aInput) and r15 (aScalar) unwritten on every path',
                  f'{sid}: no write through the request pointer %r12 on any path', f'{sid}: path instruction count',
                  f'{sid}: exact bytes of every pre-window path instruction',
                  f'{sid}: exact direct callees on pre-window paths (symbol and bytes)']
    names += [f'{k} == reviewed required sites ({len(contract.REQUIRED_SITES)})'
              for k in ('eligibility locators', 'context bindings', 'report sites extracted', 'graph locks')]
    names += [INVENTORY_CHECK, 'source definitions derived from the exact header', 'source data-member layouts']
    for sid in contract.REQUIRED_SITES:
        names += [f'{sid}: site fields equal the canonical contract', f'{sid}: finite proof',
                  f'{sid}: arithmetic graph fingerprint', f'{sid}: graph node count', f'{sid}: graph source',
                  f'{sid}: source definition hash']
    return names
