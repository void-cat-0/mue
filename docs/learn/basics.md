# State and lists

[Learn contents][learn-index]

Focus can count down, but it forgets everything when a session ends. In this
chapter you split the timer into parts that share state, keep a list of
finished sessions, and hide that list behind a toggle.

## Share state between parts

When two parts of a view need the same value, create the signal in their
common parent and hand it down. A part that only *reads* the value takes it as
a normal parameter. A part that also *writes* it marks the parameter
`#[model]`:

```rust
use mue::{components::use_interval, prelude::*};
use std::rc::Rc;

#[component]
fn clock_face(remaining: u32) -> impl Component {
    let label = remaining.map(|s| Rc::<str>::from(format!("{:02}:{:02}", s / 60, s % 60)));
    text(label).style(style! { font-size: 64px; color: #2d2a26; })
}

#[component]
fn start_button(#[model] running: bool) -> impl Component {
    button(running.pick("Pause", "Start"))
        .primary(true)
        .on_activate(move || running.update(|on| *on = !*on))
}

#[component]
fn timer() -> impl Component {
    let running = signal(false);
    let remaining = signal(25 * 60_u32);
    use_interval(1., move || {
        if running.get() && remaining.get() > 0 {
            remaining.update(|s| *s -= 1);
        }
    });

    vstack().gap(16.).style(style! { align-items: center; }).children((
        clock_face(remaining),
        start_button(running),
    ))
}
```

Inside `clock_face`, `remaining` is a `Prop<u32>`: it can be a fixed number or
follow a signal, and `map` works on both. `clock_face(1500)` shows a fixed
time; `clock_face(remaining)` follows the timer. Inside `start_button`,
`running` is a writable `Signal<bool>` that belongs to the caller, so the
button changes the parent's state directly.

| To read a signal | Use |
| --- | --- |
| `remaining.get()` | The current value, for `Copy` types, inside a handler. |
| `history.get_clone()` | The current value of a type that must be cloned, such as a list. |
| `remaining.map(...)` | A value derived from one signal that stays up to date. |
| `computed(move \|_\| ...)` | A value derived from several signals; read them inside the closure. |

## Keep a list of sessions

A finished session should appear in a history list. Store the list in a signal
holding an `Rc<[Session]>`, and render it with `map_keyed`, which builds one
row per item and identifies each item by a key:

<div class="mue-demo">
<div class="mue-demo-code">

```rust
use mue::{prelude::*, renderer::node::map_keyed};
use std::rc::Rc;

#[derive(Clone, PartialEq)]
struct Session {
    id: u32,
    minutes: u32,
}

#[component]
fn history() -> impl Component {
    let sessions = signal(Rc::<[Session]>::from(vec![]));
    let next_id = signal(1_u32);

    let total = sessions.map(|list| {
        let minutes: u32 = list.iter().map(|s| s.minutes).sum();
        Rc::<str>::from(format!("{minutes} minutes today"))
    });
    let rows = map_keyed(sessions, |s| s.id, |s| {
        text(Rc::<str>::from(format!("Session {}: {} min", s.id, s.minutes)))
            .style(style! { color: #2d2a26; })
    });

    vstack().gap(8.).children((
        text(total).style(style! { font-weight: 700; color: #2d2a26; }),
        hstack().gap(8.).children((
            button("Finish session").on_activate(move || {
                let id = next_id.get();
                next_id.set(id + 1);
                let mut list = sessions.get_clone().to_vec();
                list.push(Session { id, minutes: 25 });
                sessions.set(list.into());
            }),
            button("Clear").on_activate(move || sessions.set(Rc::from(vec![]))),
        )),
        rows,
    ))
}
```

</div>
<figure class="mue-demo-preview mue-focus">

![A total of 75 minutes above three session rows][shot-guide-history]

</figure>
</div>

To change the list, build a new one and `set` it. mue compares the old and new
keys: rows whose key is still there are kept (and moved if the order changed),
new keys get new rows, and missing keys are removed. Keys must be unique; an ID
is the usual choice.

Use a tuple for a fixed set of different children, like `(text(total),
hstack()...)` above, and a `Vec` or `map_keyed` when the number of children
changes.

## Update a row in place

`map_keyed` builds each row from the item it first sees. If items can change
while keeping their key, use `map_keyed_prop`, which gives the row the current
value and index:

```rust
use mue::{prelude::*, renderer::node::map_keyed_prop};
use std::rc::Rc;

#[component]
fn history_rows(sessions: Rc<[(u32, u32)]>) -> impl Component {
    let rows = map_keyed_prop(sessions, |(id, _)| *id, |row| {
        let label = computed(move |_| {
            let (id, minutes) = row.value.get();
            Rc::<str>::from(format!("#{} · session {id} · {minutes} min", row.index.get() + 1))
        });
        text(label).style(style! { color: #2d2a26; })
    });
    vstack().gap(4.).children(rows)
}
```

If a session's minutes change from 25 to 50, its row updates without being
rebuilt.

## Conditional children

The history should only show when you ask for it. `show_if` keeps a child
mounted but shows it only while a condition holds:

```rust
use mue::{prelude::*, renderer::node::show_if};

#[component]
fn history_panel() -> impl Component {
    let open = signal(false);
    vstack().gap(8.).children((
        button(open.pick("Hide history", "Show history"))
            .on_activate(move || open.update(|o| *o = !*o)),
        show_if(open, text("Your finished sessions appear here.").style(style! { color: #2d2a26; })),
    ))
}
```

Because the hidden child stays mounted, its own state survives: collapse the
history and open it again, and the list is still there. When a child should
start fresh each time it appears, or play an animation as it comes and goes,
use `presence`, which you will meet in the next chapter.

## What you learned

- Create shared state in the closest common parent; pass read-only values as
  parameters and writable ones with `#[model]`.
- Store lists as `Rc<[T]>` in a signal and render them with `map_keyed`.
- Use `map_keyed_prop` when items change while their keys stay the same.
- `show_if` hides a child without losing its state.

The [reactive runtime reference][state-reference] covers ownership, scopes,
and keyed lists in detail.

**Previous** [Your first window][getting-started-guide] · **Next** [Layout, paint, and motion][style-guide]

[shot-guide-history]: ../images/guide-history.png

[learn-index]: index.md
[getting-started-guide]: first-step.md
[style-guide]: style.md
[state-reference]: ../reference/state-children-and-scope.md
