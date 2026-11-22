#!/bin/bash
W=$HOME/kicad-port/generator-refresh-57e
tail -30 $W/logs/evidence.log; echo ==; cat $W/logs/capture.log; cat $W/logs/apply-refreshed.log; tail -12 $W/logs/assemble.log; ls -la ~/kicad-port.build-lock 2>&1|head -1; ls -la $W/capture $W/final/port/extractor/generated
