"""The references' consensus: where two independent models of the same circuit agree, what they
agree on is the target, and how far ours is from it is the gap to close; where they disagree,
both are shown and the measure is flagged, to be settled by ear or by the circuit (#347,
decision 125).

A report's kind gives its measurements as records, one for each case and measure:

    {"measure": "accent, dB over the plain note", "case": "AAAA-res80", "unit": "dB",
     "values": {"ours": 4.1, "Acid V": 4.9, "ABL3": 5.2}}

The unit says how two values are compared and how near counts as agreeing (`TOLERANCES`): Hz and
ms by their ratio, in octaves, so 200 against 220 Hz is as far as 2000 against 2200, and dB,
octaves, semitones and cents by their difference. A measure the references agree on within the
tolerance has their mean as its target, a log-mean for Hz and ms; a gap is ours less the target,
and its size in tolerances ranks it against measures in other units. With one reference, it's
the target alone.
"""

from __future__ import annotations

import datetime
import html
import json
import math
from collections import defaultdict
from pathlib import Path

# How near two values must be to agree, in each unit: for Hz and ms, in octaves of their ratio.
TOLERANCES = {
    "Hz": 1.0 / 6.0,  # two semitones
    "ms": 0.25,  # about 19%
    "dB": 1.0,
    "octaves": 0.1,
    "semitones": 0.25,
    "cents": 25.0,
}
# The units compared by their ratio, in octaves.
RATIOS = {"Hz", "ms"}
# How a gap is written in each unit.
GAP_UNITS = {"Hz": "octaves", "ms": "octaves"}
# The least share of a measure's cases the references must agree on for its gap to be ranked, and
# the fewest cases, so one case they happen to agree on doesn't stand for a measure they mostly
# don't.
RANKED_SHARE = 0.25
RANKED_CASES = 2


def scaled(value: float, unit: str) -> float:
    """A value on the scale it's compared on: octaves for Hz and ms, as it is for the rest."""
    if unit in RATIOS:
        return math.log2(value) if value > 0 else float("nan")
    return value


def unscaled(value: float, unit: str) -> float:
    return 2.0**value if unit in RATIOS else value


def finite(value) -> bool:
    return value is not None and isinstance(value, (int, float)) and math.isfinite(value)


def judge(record: dict, references: list[str]) -> dict:
    """A record's verdict: whether the references agree, the target, our gap from it in the
    unit's gap scale and in tolerances, and how far apart the references are."""
    unit = record["unit"]
    tolerance = TOLERANCES[unit]
    ours = record["values"].get("ours")
    theirs = {name: record["values"].get(name) for name in references}
    present = {name: scaled(value, unit) for name, value in theirs.items() if finite(value)}
    present = {name: value for name, value in present.items() if math.isfinite(value)}
    ours_scaled = scaled(ours, unit) if finite(ours) else float("nan")
    verdict = {**record, "status": "missing", "target": float("nan"), "gap": float("nan"), "size": float("nan"), "apart": float("nan")}
    if not present or not math.isfinite(ours_scaled):
        return verdict
    values = list(present.values())
    apart = max(values) - min(values)
    verdict["apart"] = apart
    if len(present) < len(references):
        return verdict
    if len(values) > 1 and apart > tolerance:
        verdict["status"] = "disagree"
        # Ours between them is as near as can be told; otherwise, from the nearer.
        low, high = min(values), max(values)
        verdict["nearest gap"] = 0.0 if low <= ours_scaled <= high else (ours_scaled - high if ours_scaled > high else ours_scaled - low)
        verdict["nearer"] = min(present, key=lambda name: abs(present[name] - ours_scaled))
        return verdict
    target = sum(values) / len(values)
    verdict["status"] = "agree" if len(values) > 1 else "one"
    verdict["target"] = unscaled(target, unit)
    verdict["gap"] = ours_scaled - target
    verdict["size"] = abs(ours_scaled - target) / tolerance
    return verdict


def summarize(verdicts: list[dict]) -> list[dict]:
    """Each measure over its cases: how many the references agree on, our mean and largest gap
    where they do, and how far apart they are where they don't, most of a tolerance first."""
    by_measure = defaultdict(list)
    for verdict in verdicts:
        by_measure[verdict["measure"]].append(verdict)
    rows = []
    for measure, chosen in by_measure.items():
        unit = chosen[0]["unit"]
        tolerance = TOLERANCES[unit]
        agreed = [v for v in chosen if v["status"] in ("agree", "one")]
        disagreed = [v for v in chosen if v["status"] == "disagree"]
        row = {
            "measure": measure,
            "unit": unit,
            "gap unit": GAP_UNITS.get(unit, unit),
            "tolerance": tolerance,
            "cases": len(chosen),
            "agreed": len(agreed),
            "disagreed": len(disagreed),
        }
        if agreed:
            worst = max(agreed, key=lambda v: v["size"])
            row.update({
                "mean gap": sum(abs(v["gap"]) for v in agreed) / len(agreed),
                "mean signed gap": sum(v["gap"] for v in agreed) / len(agreed),
                "score": sum(v["size"] for v in agreed) / len(agreed),
                "worst gap": worst["gap"],
                "worst case": worst["case"],
                "worst": {k: worst["values"].get(k) for k in worst["values"]},
                "worst target": worst["target"],
                "within": sum(1 for v in agreed if v["size"] <= 1.0),
            })
        if disagreed:
            widest = max(disagreed, key=lambda v: v["apart"])
            row.update({
                "mean apart": sum(v["apart"] for v in disagreed) / len(disagreed),
                "widest apart": widest["apart"],
                "widest case": widest["case"],
                "widest": {k: widest["values"].get(k) for k in widest["values"]},
                "between": sum(1 for v in disagreed if v.get("nearest gap") == 0.0),
            })
        rows.append(row)
    rows.sort(key=lambda row: -row.get("score", -1.0))
    return rows


def ranked(row: dict) -> bool:
    """Whether a measure's gap is ranked: the references agree on enough of its cases."""
    return row.get("agreed", 0) >= max(RANKED_CASES, RANKED_SHARE * row["cases"]) or (row["cases"] == 1 and row["agreed"] == 1)


def shown(value, unit: str, sign: bool = False, digits: int | None = None) -> str:
    if not finite(value):
        return "–"
    if digits is None:
        digits = {"Hz": 0, "ms": 1, "dB": 1, "octaves": 2, "semitones": 2, "cents": 0}.get(unit, 2)
    text = f"{value:+.{digits}f}" if sign else f"{value:.{digits}f}"
    return text.replace("-", "−")


def values_text(values: dict, unit: str, references: list[str]) -> str:
    parts = [f"ours {shown(values.get('ours'), unit)}"]
    parts += [f"{name} {shown(values.get(name), unit)}" for name in references]
    return ", ".join(parts) + f" {unit}"


def section(records: list[dict], references: list[str]) -> tuple[str, list[dict]]:
    """A report's consensus view, as HTML, and its measures' summaries."""
    verdicts = [judge(record, references) for record in records]
    rows = summarize(verdicts)
    if not rows:
        return "", rows
    two = len(references) > 1
    table_rows = []
    for row in rows:
        unit, gap_unit = row["unit"], row["gap unit"]
        flag = "⚑ " if row["disagreed"] else ""
        cells = [
            f"{flag}{html.escape(row['measure'])}",
            f"{row['agreed']} of {row['cases']}" if two else str(row["cases"]),
            shown(row.get("mean gap"), gap_unit),
            shown(row.get("mean signed gap"), gap_unit, sign=True),
            shown(row.get("score"), "", digits=2),
            html.escape(f"{row['worst case']}: {shown(row['worst gap'], gap_unit, sign=True)} {gap_unit}") if "worst case" in row else "–",
        ]
        if two:
            cells.append(
                html.escape(f"{shown(row['mean apart'], gap_unit)} {gap_unit}; widest at {row['widest case']}: {values_text(row['widest'], unit, references)}")
                if row["disagreed"]
                else ""
            )
        table_rows.append(cells)
    headers = ["Measure", "References agree on" if two else "Cases", "Our mean gap", "Signed", "In tolerances", "Our largest gap"]
    if two:
        headers.append("Where the references disagree")
    head = "".join(f"<th>{html.escape(h)}</th>" for h in headers)
    body = "".join("<tr>" + "".join(f"<td>{c}</td>" for c in cells) + "</tr>" for cells in table_rows)
    detail_rows = []
    for v in verdicts:
        unit = v["unit"]
        status = {"agree": "agree", "one": "", "disagree": "⚑ disagree", "missing": "not measured"}[v["status"]]
        detail_rows.append(
            "<tr>"
            + "".join(
                f"<td>{c}</td>"
                for c in [
                    html.escape(v["measure"]),
                    html.escape(v["case"]),
                    shown(v["values"].get("ours"), unit),
                    *[shown(v["values"].get(name), unit) for name in references],
                    shown(v["target"], unit),
                    shown(v["gap"], GAP_UNITS.get(unit, unit), sign=True),
                    status,
                ]
            )
            + "</tr>"
        )
    detail_head = "".join(f"<th>{html.escape(h)}</th>" for h in ["Measure", "Case", "Ours", *references, "Target", "Our gap", ""])
    tolerances = "; ".join(
        f"{unit} {shown(value, '', digits=3).rstrip('0').rstrip('.')}{' octaves' if unit in RATIOS else ''}"
        for unit, value in TOLERANCES.items()
        if any(r["unit"] == unit for r in rows)
    )
    reading = (
        "Each measure over the cases. Where the references agree within the tolerance, their mean "
        "is the target, and our gap is ours less it: in octaves for frequencies and times, so a "
        "gap of +0.1 is 7% higher, and in the measure's own unit for the rest. In tolerances is "
        "the mean gap's size over the tolerance, which ranks gaps in different units. A ⚑ marks a "
        "measure the references disagree on in some cases, with how far apart they are; there "
        "neither is the target." if two else
        "Each measure over the cases, against the one reference's values as the target: our gap is "
        "ours less it, in octaves for frequencies and times, and in the measure's own unit for the "
        "rest. In tolerances is the mean gap's size over the tolerance."
    )
    page = (
        f"<p>{html.escape(reading)} Tolerances: {html.escape(tolerances)}.</p>"
        f'<div class="scroll"><table><thead><tr>{head}</tr></thead><tbody>{body}</tbody></table></div>'
        f'<details><summary>Every case</summary><div class="scroll"><table><thead><tr>{detail_head}</tr></thead>'
        f"<tbody>{''.join(detail_rows)}</tbody></table></div></details>"
    )
    return page, rows


def write_index(suite: str, pages: list, path: Path, data_path: Path) -> None:
    """A suite's index page: every measure of every spec ranked by our gap from the consensus, the
    measures the references disagree on, and a link to each spec's page; and the same as JSON."""
    import report

    gaps, thin, disagreements, links, everything = [], [], [], [], []
    references = []
    for spec, page, built in pages:
        title = spec.report.get("title") or spec.path.stem
        links.append(f'<li><a href="{html.escape(page.name)}">{html.escape(title)}</a> <span class="note">{html.escape(spec.path.name)}</span></li>')
        for name in spec.reference_names:
            if name not in references:
                references.append(name)
        for row in built.get("consensus", []):
            entry = {**row, "spec": spec.path.stem, "title": title, "page": page.name}
            everything.append(entry)
            if "score" in entry and ranked(entry):
                gaps.append(entry)
            elif "score" in entry:
                thin.append(entry)
            if row["disagreed"]:
                disagreements.append(entry)
    gaps.sort(key=lambda row: -row["score"])
    disagreements.sort(key=lambda row: -(row["disagreed"] / row["cases"]) * row["mean apart"] / row["tolerance"])

    def link(row):
        return f'<a href="{html.escape(row["page"])}">{html.escape(row["spec"])}</a>'

    thin.sort(key=lambda row: -row["score"])
    gap_rows = []
    for rank, row in enumerate(gaps + thin, 1):
        unit = row["gap unit"]
        direction = "ours higher" if row["mean signed gap"] > 0 else "ours lower"
        gap_rows.append([
            str(rank) if row in gaps else "",
            link(row),
            ("⚑ " if row["disagreed"] else "") + html.escape(row["measure"]),
            f"{row['agreed']} of {row['cases']}",
            shown(row["score"], "", digits=2),
            html.escape(f"{shown(row['mean gap'], unit)} {unit}, {direction}"),
            html.escape(f"{row['worst case']}: {values_text(row['worst'], row['unit'], references)}, target {shown(row['worst target'], row['unit'])}"),
        ])
    disagreement_rows = []
    for row in disagreements:
        unit = row["gap unit"]
        disagreement_rows.append([
            link(row),
            html.escape(row["measure"]),
            f"{row['disagreed']} of {row['cases']}",
            html.escape(f"{shown(row['mean apart'], unit)} {unit}"),
            f"{row['between']} of {row['disagreed']}",
            html.escape(f"{row['widest case']}: {values_text(row['widest'], row['unit'], references)}"),
        ])

    def table(headers, rows):
        head = "".join(f"<th>{html.escape(h)}</th>" for h in headers)
        body = "".join("<tr>" + "".join(f"<td>{c}</td>" for c in cells) + "</tr>" for cells in rows)
        return f'<div class="scroll"><table><thead><tr>{head}</tr></thead><tbody>{body}</tbody></table></div>'


    tolerances = "; ".join(f"{unit} {value:.3g}{' octaves' if unit in RATIOS else ''}" for unit, value in TOLERANCES.items())
    page_text = f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Acid nuance suite</title>
<style>{report.STYLE}</style>
</head>
<body>
<main>
<h1>The {html.escape(suite)} suite</h1>
<p class="meta">{len(pages)} specs · references: {html.escape(", ".join(references))} · made {datetime.date.today().isoformat()}</p>
<h2>How to read it</h2>
<p>Every measure of every spec, over its cases. Where {html.escape(" and ".join(references))} agree
within a tolerance, their mean is the target, and our gap is ours less it: in octaves for
frequencies and times, and in the measure's own unit for the rest. The ranking is by the mean
gap's size in tolerances, so gaps in different units can be set side by side. A ⚑ marks a
measure the references disagree on in some of its cases; those cases are left out of its gap and
listed below. A measure they agree on in fewer than a quarter of its cases, or in only one, isn't
ranked, and comes after the ranked ones. Tolerances: {html.escape(tolerances)}.</p>
<h2>Our biggest gaps from the consensus</h2>
{table(["#", "Spec", "Measure", "Agreed", "In tolerances", "Our mean gap", "Largest, with every side"], gap_rows)}
<h2>Where the references disagree</h2>
<p>The share of a measure's cases they disagree on, how far apart they are there, and how many of
those cases have ours between them.</p>
{table(["Spec", "Measure", "Disagree on", "Apart on average", "Ours between", "Widest"], disagreement_rows)}
<h2>The specs</h2>
<ul>{"".join(links)}</ul>
</main>
</body>
</html>
"""
    path.write_text(page_text)
    data_path.write_text(json.dumps({"gaps": gaps, "unranked": thin, "disagreements": disagreements, "measures": everything}, indent=1, default=float))
