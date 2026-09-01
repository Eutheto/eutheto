# Phase-02 school foundation corpus

Synthetic contract fixtures only; these files are not an official school pack or UI.

`mini-school-v1.json` has one stable section, teacher, cohort, lecture occurrence, lab occurrence, two meeting patterns, and three rooms. The lecture must precede the lab by at least two periods. Room capacity/equipment, exact required occurrence count, teacher/cohort/room no-overlap, and bidirectional occurrence-to-room links apply. The pattern-choice formulation selects one pre-enumerated pattern plus eligible rooms. The occurrence formulation selects exactly one eligible period-room pair per required occurrence. Exhaustive enumeration must produce the identical eight projected meeting/room schedules and scores.

`mini-school-infeasible-v1.json` raises section size beyond every room capacity, so both formulations enumerate no feasible schedule. The integration corpus also mutates candidates to prove missing occurrences, double room selections, floating room claims, unlinked meetings, collisions, wrong equipment, wrong counts, and reversed/insufficient lecture-lab separation are rejected.
