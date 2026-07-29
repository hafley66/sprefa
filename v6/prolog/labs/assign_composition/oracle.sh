#!/usr/bin/env bash
# oracle.sh PROGRAM SCHEDULE -- run the reference engine over a .dl6 text
# program plus a JSON arrival schedule, print the shared tick-log envelope.
LAB="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$LAB/../../compile/scripts"
swipl -q -l dl6_oracle.pl -g "oracle('$LAB/probes/$1.dl6','$LAB/probes/$2')" -g halt
