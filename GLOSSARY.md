# Glossary

Methodology vocabulary for this project's design docs. Device-specific
vocabulary (Save, Publish, Recover, Tracked, Local, …) lives in
[`CONTEXT.md`](CONTEXT.md).

## Ontology stack

Five layers, top to bottom:

1. **WHAT** — user-facing requirement; an outcome the user values.
   Lives in [`docs/qfd.md`](docs/qfd.md) §1. Phrased as a sentence.
   Example: "Sub-second visible response to typing".

2. **Function** — a transformation the device performs (verb,
   input → output). Lives in [`docs/qfd.md`](docs/qfd.md) §2's
   Functions inventory. Examples: Type, Save, Publish, Recover,
   Boot, Provision.

3. **Characteristic** ≡ **HOW** — a measurable attribute of a
   function (or of an artifact or process). Noun. Lives in
   [`docs/qfd.md`](docs/qfd.md) §2's HOW table. Examples: latency,
   reliability, durability, binary size, build time.

4. **Metric + Unit** — the quantity and scale used to express a
   characteristic's value. Examples: success rate (%), latency in
   seconds, size in MB. Packed into the target columns of §2; not
   given their own row.

5. **Target** — the value we aim for. The v0.1 / v1.0 columns of
   §2's HOW table. Examples: ≥ 95 %, ≤ 200 ms, ≤ 2 MB.

## Side layers

- **Component** — subsystem that delivers one or more
  characteristics. Lives in [`docs/qfd.md`](docs/qfd.md) §5.
- **ADR** — Architecture Decision Record ([`docs/adr.md`](docs/adr.md)).
  Captures a decision about a Component or Function with consequences.
- **Spike** — time-boxed validation experiment that returns numbers
  before integration. Referenced from
  [`docs/qfd.md`](docs/qfd.md) §6's "Watched on" column.

## How the layers connect

```
WHAT  →  Function  →  Characteristic  →  Metric+Unit  →  Target
                          ↑
                      Component  →  ADR
                          ↑
                        Spike
```

A user's **WHAT** is delivered by one or more device **Functions**;
each Function is sized by one or more **Characteristics**; each
Characteristic is quantified by a **Metric + Unit** judged against
a **Target**; each Characteristic is produced by one or more
**Components**, whose choices are recorded in **ADRs** and validated
by **Spikes**.

## Anti-patterns

Recurring drifts the docs have had to clean up. Worth naming so we
catch them early next time.

- **Solution-shape inside a WHAT or Characteristic name.** Naming a
  specific solution (`Wi-Fi`, `Ctrl-G`, `commit`, `BOM`, `monospace`,
  `e-ink`) inside an outcome or attribute. WHATs and Characteristics
  describe outcomes and attributes, not the technology that implements
  them. Move solution names to §7 tradeoffs or the relevant ADR.
  (See [`docs/qfd.md`](docs/qfd.md) §8: W13 reframe, WHAT sweep,
  H6/H7/H8/H12 sweep.)

- **Measure-vs-attribute drift.** Naming a metric (`success rate`,
  `MTBF`) where an attribute (`reliability`) would be cleaner. The
  metric belongs in the target column; the attribute belongs in
  the Characteristic name.

- **Function-vs-characteristic conflation.** Calling HOWs
  "engineering functions". A function is a transformation; a
  characteristic is one of its measurable attributes. HOWs _measure_
  functions; they are not functions. (See
  [`docs/qfd.md`](docs/qfd.md) §8 for the rename cascade.)
