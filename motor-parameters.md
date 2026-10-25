# Motor Parameters — Feed + Peel, for Coil Generator Milestones 2–4

Status key: **FIRM** = settled design decision, safe to build against.
**PLACEHOLDER** = a working default used in earlier calculations, not yet
finalized — fine to build/test against, but expect these to change before
production layout.

---

## Feed motor

| Parameter | Value | Status |
|---|---|---|
| Poles | 20 | FIRM |
| Coils | 18 | FIRM |
| Coils per phase | 6 | FIRM (= coils / 3) |
| Topology | Single stator (one PCB, one air gap) | FIRM |
| Mean radius (R) | 25 mm | FIRM |
| Active radial length (L) | 10 mm | FIRM |
| Magnet | 42NE301412-NI — 12.07mm (L) × 5.84mm (W) × 4.95mm (thick), N42, axially magnetized, oriented radially (12.07mm dimension along radius) | FIRM |
| Layers | 6 | PLACEHOLDER |
| Trace width | 0.3 mm | PLACEHOLDER |
| Trace spacing | 0.15 mm | PLACEHOLDER |
| Copper weight | 2 oz (0.07mm) | PLACEHOLDER |

**Why layers/trace/spacing are placeholder, not firm:** at these values, turns-per-coil
comes out high enough (~132) that current density lands around 30 A/mm² for the
0.1 N·m target torque — too hot for continuous operation on buried layers. The fix
is fewer turns / wider trace, but the final combination hasn't been chosen yet. Safe
to build the generator against these defaults now (the geometry math doesn't care
what the final numbers are), but don't treat 6 layers / 0.3mm / 0.15mm as the
production values — expect a follow-up with revised numbers before final layout.

**Rotor construction:** brass outer ring carrying the sprocket teeth (stiffness +
wear resistance), radial spokes or an offset flange connecting ring to hub to avoid
eddy currents in the brass, 3D-printed magnet-holder disc inboard of the brass ring.
Single air gap means continuous axial magnetic attraction toward the stator PCB —
needs a thrust bearing, not just a bushing.

**Target torque used in calculations:** 0.1 N·m (top of stated 0.05–0.1 N·m range).
**B_g assumption used in calculations:** ~0.4 T (rough estimate, not measured).

---

## Peel motor

| Parameter | Value | Status |
|---|---|---|
| Poles | 8 | FIRM |
| Coils | 9 | FIRM |
| Coils per phase | 3 | FIRM (= coils / 3) |
| Topology | Single stator, consistent with feed motor | FIRM |
| Mean radius | ~7 mm | FIRM |
| Motor OD | ~20 mm | FIRM |
| Magnet target size | ~5mm (radial) × 3mm (circumferential) × 2mm (thick) | PLACEHOLDER — no specific SKU sourced yet, target dimensions only |
| Layers | 4 | FIRM |
| Trace width | Not yet set — start around 0.15mm as a working default | PLACEHOLDER |
| Trace spacing | Not yet set — start around 0.1mm as a working default | PLACEHOLDER |

**Why the trace width/spacing are unset:** coil pitch here is only ~4.9mm (much
tighter than the feed motor's), so this motor is closer to fab-minimum trace/space
constraints than electromagnetics constraints. Worth having the generator itself
flag if a requested turns/trace/space combination doesn't leave room to route,
rather than assuming it fits.

**Target torque used in calculations:** 0.001–0.007 N·m (EIA-481 cover-tape peel
force range, 10mm wheel radius — no actual tape spec measured yet, so treat this
range as provisional).

**Wheel radius mismatch:** peel wheel (10mm) is smaller than the feed sprocket
radius, so synchronized motion means matching linear tape speed, not matching
angular velocity — ω_peel × r_peel = ω_feed × r_feed. This is a firmware/motion-
profile concern, not a coil-generator concern, but worth the session knowing it
exists since it'll come up when the two motors' control loops are wired together.

---

## Board-level / shared

| Parameter | Value | Status |
|---|---|---|
| Number of motor islands on shared PCB | 2 (feed + peel) | FIRM |
| Magnetic isolation spacing guideline | ≥ ~3× larger motor's outer radius, center-to-center (~90–100mm for the feed motor's 32mm outer radius) | FIRM as a rule of thumb, not yet checked against actual board footprint |
| Fallback if spacing doesn't fit | Thin steel/mu-metal shield strip between motor islands | PLACEHOLDER — only needed if spacing check fails |
| Board placement (x, y per motor) | Not yet determined | **TBD** — no physical board layout has been drawn yet |
| Via drill / annular ring | Not yet specified | **TBD** — needs a value confirmed against target fab's capability before this is meaningful |
| Winding/phase convention | Coil index mod 3 → phase group; per-coil sign flip so same-phase coils reinforce (same logic validated in the pole/coil animation tool) | FIRM, applies to both motors identically |

**Two open items the session will need answered before Milestone 3/4 can go to
real layout, not just geometry math:** actual (x, y) placement for each motor
island on the shared board, and via drill/annular ring sized against the real
target fab. Neither has a value yet — worth flagging back rather than guessing.
