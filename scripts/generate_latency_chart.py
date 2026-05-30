from pathlib import Path

import numpy as np
import matplotlib.pyplot as plt

from _plot_common import (
    ASSETS_DIR,
    IMPL_COLORS,
    IMPLEMENTATIONS,
    IMPL_LABELS,
    LATENCY_SIZES,
    apply_axis_style,
    load_criterion_mean_ns,
    save_svg,
)


def plot_mean_latency_by_size(assets_dir: Path):
    """Criterion-mean per-lookup latency vs map size. Linear y, categorical x.

    Cache-hierarchy cliffs (L1→L2→L3→DRAM) appear as visible jumps; absolute
    ns/op is readable directly.
    """
    labels: list[str] = []
    means: dict[str, list[float]] = {impl: [] for impl in IMPLEMENTATIONS}

    for size_label in LATENCY_SIZES:
        group = f"get_hit_latency_{size_label}"
        # Bench id is `<group>_<impl>` (see benches/README.md).
        try:
            row = {
                impl: load_criterion_mean_ns(group, f"{group}_{impl}")
                for impl in IMPLEMENTATIONS
            }
        except FileNotFoundError:
            continue
        labels.append(size_label)
        for impl, mean_ns in row.items():
            means[impl].append(mean_ns)

    if not labels:
        print("no Criterion latency data found, skipping mean-latency plot")
        return

    x = np.arange(len(labels))
    fig, ax = plt.subplots(figsize=(10, 6), constrained_layout=True)
    for impl in IMPLEMENTATIONS:
        ax.plot(
            x,
            means[impl],
            color=IMPL_COLORS[impl],
            linewidth=2,
            label=IMPL_LABELS[impl],
        )

    ax.set_xticks(x)
    ax.set_xticklabels(labels, fontsize=12)
    apply_axis_style(
        ax,
        title="Get-Hit Latency vs Map Size",
        subtitle="Mean per get() — lower is better",
        xlabel="Map size (entries)",
        ylabel="Latency per lookup (ns)",
        y_formatter=lambda v, _: f"{v:.0f}",
    )
    ax.legend(fontsize=12)
    save_svg(fig, assets_dir / "benchmark-latency.svg")


def main():
    plot_mean_latency_by_size(ASSETS_DIR)


if __name__ == "__main__":
    main()
