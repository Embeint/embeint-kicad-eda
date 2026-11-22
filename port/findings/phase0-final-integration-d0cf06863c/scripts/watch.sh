#!/usr/bin/env bash
# Emit stage stamps and failure signatures of the detached proof; exit when it ends.
rev=${1:?revision}
work="$HOME/kicad-port/phase0-final-$rev"
pid=$(cat "$work/run.pid")
seen=$(wc -l < "$work/run.log")
while true; do
    total=$(wc -l < "$work/run.log")
    if (( total > seen )); then
        sed -n "$((seen + 1)),${total}p" "$work/run.log" | sed 's/\x1b\[[0-9;]*m//g' |
            grep -E '^== |error(\[|:)|FAILED|Traceback|AssertionError|panicked|test result: FAILED|Errors while running CTest|PHASE0_INTEGRATION_PROOF_COMPLETE' || true
        seen=$total
    fi
    if ! kill -0 "$pid" 2>/dev/null; then
        echo "proof process exited; done-all present: $([[ -f $work/done-all ]] && echo yes || echo no)"
        tail -5 "$work/run.log" | sed 's/\x1b\[[0-9;]*m//g'
        exit 0
    fi
    sleep 20
done
