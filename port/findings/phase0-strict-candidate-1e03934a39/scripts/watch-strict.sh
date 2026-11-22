#!/usr/bin/env bash
# Emit stage stamps and failure signatures of the detached strict run; exit when it ends.
rev=${1:?revision}
work="$HOME/kicad-port/phase0-strict-candidate-$rev"
pid=$(cat "$work/run.pid")
seen=0
while true; do
    total=$(wc -l < "$work/run.log")
    if (( total > seen )); then
        sed -n "$((seen + 1)),${total}p" "$work/run.log" | sed 's/\x1b\[[0-9;]*m//g' |
            grep -E '^== |error(\[|:)|FAILED|Traceback|AssertionError|panicked|Phase 0:|busy' || true
        seen=$total
    fi
    if ! kill -0 "$pid" 2>/dev/null; then
        echo "strict run exited; done-all present: $([[ -f $work/done-all ]] && echo yes || echo no)"
        tail -3 "$work/run.log" | sed 's/\x1b\[[0-9;]*m//g'
        exit 0
    fi
    sleep 30
done
