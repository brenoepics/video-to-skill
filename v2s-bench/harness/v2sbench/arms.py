"""Experimental arms.

An arm is *only* a choice of context material. Everything else about a
trial -- model, tools, sandbox, turn and time budget, task prompt -- is
held identical by construction, because the moment an arm differs in any
other way the comparison stops meaning anything.

See ``docs/preregistration.md`` section 3.
"""

from __future__ import annotations

from dataclasses import dataclass, field

MaterialKind = str


@dataclass(frozen=True)
class Arm:
    """One experimental condition."""

    id: str
    name: str
    material: MaterialKind
    """Which artifact gets injected; ``none`` means the baseline."""
    isolates: str
    """The confound or component this arm exists to separate out."""
    description: str
    requires_artifact: bool = True
    """False only for A0, which needs no material built for it."""


A0 = Arm(
    id="A0",
    name="baseline",
    material="none",
    isolates="(reference condition)",
    requires_artifact=False,
    description=(
        "A competent prompt and nothing else. Keeps every tool the other arms "
        "have, including documentation reading and exploratory commands, so the "
        "baseline is allowed to discover the procedure on its own."
    ),
)

A1 = Arm(
    id="A1",
    name="v2s-full",
    material="v2s-skill",
    isolates="the product as shipped",
    description=(
        "The compiled v2s skill with pixel-read commands, per-step timestamp "
        "provenance, and the verification badge written by a sandboxed execution pass."
    ),
)

A2 = Arm(
    id="A2",
    name="transcript-only",
    material="transcript-skill",
    isolates="pixel grounding",
    description=(
        "A skill compiled from the ASR transcript alone, with frame inspection "
        "withheld. Isolates the project's central claim that commands must be read "
        "off pixels rather than paraphrased from narration."
    ),
)

A3 = Arm(
    id="A3",
    name="v2s-unverified",
    material="v2s-skill-unverified",
    isolates="execution verification and repair",
    description=(
        "The same extraction as A1 with `vts-extract verify` not run, so no step was "
        "executed and no repair loop corrected it. Isolates what verification buys "
        "downstream, as opposed to what it buys in confidence."
    ),
)

A4 = Arm(
    id="A4",
    name="human-expert",
    material="human-skill",
    isolates="the ceiling",
    description=(
        "A skill hand-written from the same video by an author who has not read the "
        "task checker. Bounds how much of the remaining gap is extraction error "
        "rather than the task simply being hard."
    ),
)

A5 = Arm(
    id="A5",
    name="placebo",
    material="placebo-skill",
    isolates="context length, provenance formatting, procedure priming",
    description=(
        "The real v2s pipeline run on a DIFFERENT video from the same tool family: "
        "genuine extraction, genuine [t=MM:SS] citations, a genuine verification "
        "badge, a procedure that does not solve this task. Matched to A1 on length, "
        "register, formatting and provenance decoration, differing only in whether "
        "the content is the right content.\n\n"
        "Deliberately not random or irrelevant filler: topically-adjacent irrelevant "
        "documents have been measured to IMPROVE performance while random text helps "
        "least, so a noise placebo would understate the very confound it exists to "
        "bound. If A5 recovers most of A1's gain over A0, the effect was never about "
        "what the video taught."
    ),
)

A6 = Arm(
    id="A6",
    name="raw-transcript",
    material="raw-transcript",
    isolates="the cheap alternative",
    description=(
        "The raw ASR transcript pasted into context, unprocessed. Answers the "
        "obvious objection: why compile a skill at all when you could paste the "
        "transcript? Published work that holds the source information fixed and "
        "varies only the packaging finds a much smaller effect than skill-vs-nothing "
        "(single-digit points, against roughly +17 for skill-vs-nothing), so this is "
        "the comparison the benchmark will actually be judged on -- and the one that "
        "sets the sample size."
    ),
)

A7 = Arm(
    id="A7",
    name="v2s-forced",
    material="v2s-skill",
    isolates="skill discovery",
    description=(
        "The same skill as A1, but invoked explicitly rather than left to be "
        "discovered from its description. A1 is the realistic condition; A7 is the "
        "upper bound if discovery were perfect. The gap between them is a property "
        "of the generated `description` field, not of the procedure, and it is "
        "separately actionable: a skill that is never triggered cannot help however "
        "good it is."
    ),
)

ALL_ARMS: tuple[Arm, ...] = (A0, A1, A2, A3, A4, A5, A6, A7)
BY_ID: dict[str, Arm] = {a.id: a for a in ALL_ARMS}

#: The headline comparison. Cheap enough to run over the whole suite.
ARMSET_CORE = ("A0", "A1")

#: The comparisons that distinguish v2s from a transcript summariser.
#: Run on a subset; this is where the interesting hypotheses live.
ARMSET_ABLATION = ("A0", "A1", "A2", "A3", "A5", "A6")

#: The packaging comparison alone: same information, different wrapping.
#: The cheapest way to test the claim most likely to be challenged.
ARMSET_PACKAGING = ("A1", "A6")

#: Everything, including the human ceiling and the discovery bound. Expensive.
ARMSET_FULL = ("A0", "A1", "A2", "A3", "A4", "A5", "A6", "A7")

ARMSETS: dict[str, tuple[str, ...]] = {
    "core": ARMSET_CORE,
    "ablation": ARMSET_ABLATION,
    "packaging": ARMSET_PACKAGING,
    "full": ARMSET_FULL,
}


@dataclass(frozen=True)
class Comparison:
    """A pre-registered hypothesis test between two arms."""

    hypothesis: str
    treatment: str
    reference: str
    claim: str
    fails_if: str
    corrected: bool = False
    """Whether this comparison is in the Holm-Bonferroni family of ablations."""


#: Fixed before any trial runs. ``stats.analysis`` tests exactly these and
#: nothing else; anything found later is exploratory and labelled as such.
COMPARISONS: tuple[Comparison, ...] = (
    Comparison(
        hypothesis="H1",
        treatment="A1",
        reference="A0",
        claim="A v2s skill raises task success over a well-prompted baseline.",
        fails_if="the 95% CI for the paired difference includes 0",
    ),
    Comparison(
        hypothesis="H3",
        treatment="A1",
        reference="A2",
        claim="Reading pixels beats reading the transcript.",
        fails_if="the CI includes 0 -- the transcript was enough",
        corrected=True,
    ),
    Comparison(
        hypothesis="H4",
        treatment="A1",
        reference="A3",
        claim="Execution verification contributes beyond extraction.",
        fails_if="the CI includes 0 -- the badge buys nothing measurable downstream",
        corrected=True,
    ),
    Comparison(
        hypothesis="H5",
        treatment="A1",
        reference="A5",
        claim="The benefit is not a context-length or priming artifact.",
        fails_if="the placebo recovers most of A1's gain over A0",
        corrected=True,
    ),
    Comparison(
        hypothesis="H6",
        treatment="A1",
        reference="A6",
        claim="A compiled skill beats pasting the raw transcript.",
        fails_if="the CI includes 0 -- paste the transcript instead",
        corrected=True,
    ),
    Comparison(
        hypothesis="H8",
        treatment="A7",
        reference="A1",
        claim=(
            "Explicit invocation beats ambient discovery -- i.e. there is a "
            "discovery gap, and the generated description is leaving value on "
            "the table."
        ),
        fails_if="the CI includes 0 -- discovery is already working",
        corrected=True,
    ),
)


@dataclass
class ArmMaterial:
    """The exact bytes an arm injects, plus the audit trail for them."""

    kind: MaterialKind
    skill_dir: str | None = None
    text: str | None = None
    sha256: str | None = None
    chars: int | None = None
    verified_steps: str | None = None
    blind_check: str = "skipped"
    extra: dict = field(default_factory=dict)


def resolve(arm_id: str) -> Arm:
    try:
        return BY_ID[arm_id]
    except KeyError:
        raise KeyError(f"unknown arm {arm_id!r}; known: {sorted(BY_ID)}") from None


def armset(name_or_ids: str) -> tuple[str, ...]:
    """Accept a named armset (``core``) or a comma list (``A0,A1,A6``)."""
    if name_or_ids in ARMSETS:
        return ARMSETS[name_or_ids]
    ids = tuple(part.strip() for part in name_or_ids.split(",") if part.strip())
    for i in ids:
        resolve(i)
    if not ids:
        raise ValueError("empty armset")
    return ids
