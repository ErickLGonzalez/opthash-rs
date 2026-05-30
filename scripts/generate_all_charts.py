from generate_latency_chart import plot_mean_latency_by_size
from generate_python_chart import plot_python_speedup
from generate_speedup_chart import plot_throughput_speedup
from _plot_common import ASSETS_DIR


def main():
    plot_throughput_speedup(ASSETS_DIR)
    plot_mean_latency_by_size(ASSETS_DIR)
    plot_python_speedup(ASSETS_DIR)


if __name__ == "__main__":
    main()
