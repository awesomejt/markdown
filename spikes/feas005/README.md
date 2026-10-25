# FEAS-005 GTK rendering probe

This disposable GTK 4/gtk-rs prototype converts pulldown-cmark events into a
read-only native `GtkTextView`. It exercises native layout, selection,
scrolling, scaling, and the GTK accessibility role without establishing the
production Linux application.

```sh
cargo build --locked --release --manifest-path spikes/feas005/Cargo.toml
spikes/feas005/target/release/linux-native-rendering-probe \
  fixtures/markdown/foundation/gfm-tour.md
```

For an automatically closing measurement after the end-scroll settles (or 120
frame-clock ticks):

```sh
spikes/feas005/target/release/linux-native-rendering-probe \
  build/fixtures/performance/prose-1mib.md --automated
```

The probe intentionally exposes the main tradeoff to measure: `GtkTextView`
provides continuous native selection and a text accessibility surface, but its
buffer is monolithic and specialized blocks such as native tables would need
child anchors or adjacent widgets. The results must therefore cover both small
semantic cases and the 1 MiB/5 MiB profiles before this becomes a platform
recommendation.

Run the recorded performance sequence with:

```sh
spikes/feas005/measure.sh
```

The checked-in [result summary](results-2026-10-24.yaml) shows that small and
1 MiB first paint is viable on the named machine, but far-scroll does not
converge for the large profiles and a 1 MiB logical line blocks end-scroll for
more than ten seconds. The production recommendation is therefore GTK 4 with
a virtualized semantic-block renderer, not one unbounded text buffer.
