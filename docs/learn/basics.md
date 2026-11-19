# Reactive state and children

[Learn contents][learn-index]

## Decide who owns the state

In the previous chapter, a component owned the playback flag. Now start with a control that can add an item to a queue or reset it from another button. Put the state above both controls so they read and write the same value.

```rust
use mue::prelude::*;

#[component]
fn queue_controls() -> impl Component {
    let count = signal(0_u32);
    let label = count.map(|n| format!("{n} queued"));
    let empty = count.map(|n| n == 0);

    vstack().gap(12.).children((
        text(label),
        hstack().gap(8.).children((
            button("Queue track").on_activate(move || count.update(|n| *n += 1)),
            button("Clear").disabled(empty).on_activate(move || count.set(0)),
        )),
    ))
}
```

Mount `queue_controls()` in the window from the previous chapter. Press **Queue track** twice, then **Clear**. Both the label and the button's enabled state come from `count`. If you mount two copies, each copy owns a different `count`. Move the signal to a shared parent when two groups should edit one queue.

| Read | Use it for |
| --- | --- |
| `count.get()` | A snapshot of a `Copy` value, such as a limit check inside a button handler. |
| `queue.get_clone()` | A snapshot of an owned value, such as a list you will send to a file writer. |
| `count.map(...)` | A derived value that can follow one source. |
| `computed(move \|_\| ...)` | A derived value that can track several sources. Put the reads inside the closure. |

When you create a binding during a component build, its owning node scope keeps it alive. If a host creates a signal before mounting any nodes, use `OwnedScope` and keep the owner alive for the lifetime of the view. The next example uses a tuple of mixed builders. For a list whose membership changes, give each row a key.
## Lift state into the caller

Move the count out of the controls when another part of the view needs to read it. The parent owns the signal and supplies it to the interactive child with `#[model]`. The summary gets a read-only projection from the same source.

```rust
use mue::prelude::*;

#[component]
fn queue_actions(#[model] count: u32) -> impl Component {
    hstack().gap(8.).children((
        button("Queue track").on_activate(move || count.update(|n| *n += 1)),
        button("Clear").on_activate(move || count.set(0)),
    ))
}

#[component]
fn queue_panel() -> impl Component {
    let count = signal(0_u32);
    let summary = count.map(|n| format!("{n} tracks queued"));
    vstack().gap(8.).children((text(summary), queue_actions(count)))
}
```

Mount `queue_panel()` and add a second `queue_actions(count)` to its children. Both sets of controls affect the same summary. In the next section, a list signal replaces the scalar count when you need a node for each record.

## Keep a list stable by key

You can pass a tuple for a fixed group of mixed child builders, or a vector for a variable number of one builder type. For a list that changes over time, hand `map_keyed` an owned snapshot and a stable key for each item.

```rust
use mue::{prelude::*, renderer::node::map_keyed};
use std::rc::Rc;

#[component]
fn stations() -> impl Component {
    let ids = signal(Rc::<[&'static str]>::from(["Ridge", "Creek"]));
    let rows = map_keyed(ids, |id| *id, |id| text(*id));

    vstack().children((
        button("Reverse stations").on_activate(move || {
            ids.set(ids.get_clone().iter().rev().copied().collect());
        }),
        rows,
    ))
}
```

Reversing the list moves each existing node to its new position. If a row has a local count, that count follows its key. Removing a key drops the node and its state. Use unique IDs when row names can repeat. For records whose content changes while their IDs stay fixed, use `map_keyed_prop` and bind each row to its current item value. The simpler `map_keyed` uses the item to construct the node the first time the key appears.

## Edit a record without changing its key

Use `map_keyed_prop` when the same ID can point to new values. Bind a retained row to `row.value` and `row.index`. The following button renames the first file and moves it to the end in a single write.

```rust
use std::rc::Rc;
use mue::{prelude::*, renderer::node::map_keyed_prop};

#[component]
fn exports() -> impl Component {
    let files = signal(Rc::<[(u32, &'static str)]>::from([
        (8, "draft.csv"), (12, "summary.csv"),
    ]));
    let rows = map_keyed_prop(files, |file| file.0, |row| {
        let label = computed(move |_| {
            let (_, name) = row.value.get();
            format!("{}  {name}", row.index.get() + 1)
        });
        text(label)
    });
    vstack().gap(8.).children((
        button("Publish draft").on_activate(move || {
            files.set(Rc::from([(12, "summary.csv"), (8, "final.csv")]));
        }),
        rows,
    ))
}
```

Row 8 keeps its node. Its label changes to `2  final.csv`. `map_keyed` could move that node but would still display the original title if its label came from the creation value. For a row that edits a record, capture its key and look up the record when a button runs. An index can change after construction.

## Conditional children

`show_if` retains a child while hiding it from the displayed tree. Use it for a details section whose local input should survive a collapse. Build another node when showing the section should start a fresh session. Keep a details panel mounted to preserve its local count. Open it, record a sample, close it, and reopen it. The label should still show one sample.

```rust
use mue::{prelude::*, renderer::node::show_if};

#[component]
fn audit_details() -> impl Component {
    let samples = signal(0_u32);
    vstack().gap(8.).children((
        text(samples.map(|n| format!("Samples recorded: {n}"))),
        button("Record sample").on_activate(move || samples.update(|n| *n += 1)),
    ))
}

#[component]
fn audit_panel() -> impl Component {
    let expanded = signal(false);
    vstack().gap(8.).children((
        button(expanded.pick("Hide details", "Show details"))
            .on_activate(move || expanded.update(|open| *open = !*open)),
        show_if(expanded, audit_details()),
    ))
}
```

An overlay has a different job. Use `overlay_host` for content that draws above the rest of the view. Bind an open flag and connect `.on_dismiss(...)` to close it when `.dismissible(true)` is set. For a Scene displaying gameplay, the Scene must also decide whether the raw input batch can reach gameplay. See [Scenes and input][scene-input-guide].

See the [state, children, and scope reference][state-reference] for ownership and keyed-list contracts.

**Previous** [Your first window][getting-started-guide] · **Next** [Layout, paint, and motion][style-guide]

[learn-index]: index.md
[getting-started-guide]: first-step.md
[style-guide]: style.md
[scene-input-guide]: input.md
[state-reference]: ../reference/state-children-and-scope.md
