title = SONG LIBRARY
summary =
    { $count ->
        [one] { $count } song
       *[other] { $count } songs
    } · { $selected ->
        [0] No song selected
       *[other] Selected song: { $selected }
    } · Artwork expansion / retained list state
open-artwork = Click the artwork to open details
shortcuts = Wheel / drag / arrows: scroll    1–3: open    Shift+1–3: open with dialog    S: settings    Esc: back    Q: exit
