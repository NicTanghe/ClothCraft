# GPU playback render spikes

## Reported behavior

During GPU cloth simulation, the viewport can show sudden render-time spikes.
Pausing and continuing with a different solver can clear the symptom. The
report did not include a GPU trace, scene, adapter or render timing breakdown.

## Findings

The fitting worker keeps an independent fixed-step clock. Before this change,
both CPU and GPU playback could retain up to four time steps of deadline debt.
If a GPU step ran long, the worker could immediately submit another step and
continue doing so while behind. Each resident WGSL AVBD step submits a batch
of compute dispatches and waits for an accepted-state readback on the worker.
The renderer has a separate wgpu device, but graphics and compute can compete
for the same physical adapter. The one-snapshot UI mailbox coalesces results
and does not limit the worker's GPU submission rate.

Pausing stops new playback steps after any in-flight step. Changing solver
also advances the generation, discards stale results and resets resident state
when the next generation starts. These behaviors fit the report, but cannot
distinguish queue contention from a solver-state cost or a long individual GPU
submission. Candidate growth after contact overflow is one such cost worth
checking in a trace. Hardware queue contention is a hypothesis. The overdue
worker cadence is confirmed in the scheduling code and a deterministic test.

## Fix

When GPU playback misses its next deadline, drop the overdue wall-clock
schedule and defer the next step by half the configured time step, capped at
8 ms. The worker waits through its normal interruptible control channel, so
pause, solver changes and explicit Step can still be processed promptly. The
solver retains its physical time step, iterations and accepted state. On-time
GPU work keeps the existing phase. CPU playback keeps bounded catch-up.

The tradeoff is reduced simulated time per wall second during sustained GPU
overload. The gap gives graphics an opportunity to submit work between steps.
A single expensive GPU submission can still delay a frame.

## Validation and reproduction

The deterministic test models an 80 ms step at a 16 ms time step. The previous
schedule remains overdue and allows the next step immediately. The revised GPU
schedule waits 8 ms, then returns to the normal cadence after an on-time step.
The tests also cover the CPU schedule, a shorter time step and the cap.

For hardware comparison, run the same saved project, adapter, solver, shadow
settings and camera at the base and PR revisions.

```sh
cargo run --release --locked -j 2 --bin cloth-fit -- \
  --project path/to/fitting-project.json --steps 0 --gpu-sim \
  --ui-perf-smoke --output output/gpu-playback
```

Compare the frame interval p95, p99 and maximum in `ui-perf.json` alongside
simulated time per wall second. The updated report includes p99 and maximum so
isolated stalls are visible. For intermittent spikes, capture a longer native
session and correlate graphics queue wait with GPU step time around an incident.
Frame intervals measure delivered window cadence, including scheduling delays.
They cannot attribute a stall to a particular GPU pass. No hardware
before/after timing is claimed in this report.
