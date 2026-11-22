#!/bin/bash
W=$HOME/kicad-port/generator-refresh-57e
pid=$(cat $W/run.pid)
while kill -0 $pid 2>/dev/null; do sleep 15; done
tail -8 $W/logs/run.log; ls $W/stages; test -f $W/completed.txt && echo COMPLETED || { echo FAILED; tail -20 $W/logs/evidence.log $W/logs/fixtures.log $W/logs/candidates.log 2>/dev/null; }
