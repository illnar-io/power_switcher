from evdev import ecodes as e

# Per-layout character tables: keycode → (lower, upper)
# Each layout defines what characters a physical key produces in that layout.
# Source: /usr/share/X11/xkb/symbols/<layout>

EN: dict[int, tuple[str, str]] = {
    e.KEY_Q:          ('q', 'Q'),
    e.KEY_W:          ('w', 'W'),
    e.KEY_E:          ('e', 'E'),
    e.KEY_R:          ('r', 'R'),
    e.KEY_T:          ('t', 'T'),
    e.KEY_Y:          ('y', 'Y'),
    e.KEY_U:          ('u', 'U'),
    e.KEY_I:          ('i', 'I'),
    e.KEY_O:          ('o', 'O'),
    e.KEY_P:          ('p', 'P'),
    e.KEY_LEFTBRACE:  ('[', '{'),
    e.KEY_RIGHTBRACE: (']', '}'),
    e.KEY_A:          ('a', 'A'),
    e.KEY_S:          ('s', 'S'),
    e.KEY_D:          ('d', 'D'),
    e.KEY_F:          ('f', 'F'),
    e.KEY_G:          ('g', 'G'),
    e.KEY_H:          ('h', 'H'),
    e.KEY_J:          ('j', 'J'),
    e.KEY_K:          ('k', 'K'),
    e.KEY_L:          ('l', 'L'),
    e.KEY_SEMICOLON:  (';', ':'),
    e.KEY_APOSTROPHE: ("'", '"'),
    e.KEY_Z:          ('z', 'Z'),
    e.KEY_X:          ('x', 'X'),
    e.KEY_C:          ('c', 'C'),
    e.KEY_V:          ('v', 'V'),
    e.KEY_B:          ('b', 'B'),
    e.KEY_N:          ('n', 'N'),
    e.KEY_M:          ('m', 'M'),
    e.KEY_COMMA:      (',', '<'),
    e.KEY_DOT:        ('.', '>'),
    e.KEY_SLASH:      ('/', '?'),
    e.KEY_GRAVE:      ('`', '~'),
    e.KEY_1:          ('1', '!'),
    e.KEY_2:          ('2', '@'),
    e.KEY_3:          ('3', '#'),
    e.KEY_4:          ('4', '$'),
    e.KEY_5:          ('5', '%'),
    e.KEY_6:          ('6', '^'),
    e.KEY_7:          ('7', '&'),
    e.KEY_8:          ('8', '*'),
    e.KEY_9:          ('9', '('),
    e.KEY_0:          ('0', ')'),
    e.KEY_MINUS:      ('-', '_'),
    e.KEY_EQUAL:      ('=', '+'),
}

RU: dict[int, tuple[str, str]] = {
    e.KEY_Q:          ('й', 'Й'),
    e.KEY_W:          ('ц', 'Ц'),
    e.KEY_E:          ('у', 'У'),
    e.KEY_R:          ('к', 'К'),
    e.KEY_T:          ('е', 'Е'),
    e.KEY_Y:          ('н', 'Н'),
    e.KEY_U:          ('г', 'Г'),
    e.KEY_I:          ('ш', 'Ш'),
    e.KEY_O:          ('щ', 'Щ'),
    e.KEY_P:          ('з', 'З'),
    e.KEY_LEFTBRACE:  ('х', 'Х'),
    e.KEY_RIGHTBRACE: ('ъ', 'Ъ'),
    e.KEY_A:          ('ф', 'Ф'),
    e.KEY_S:          ('ы', 'Ы'),
    e.KEY_D:          ('в', 'В'),
    e.KEY_F:          ('а', 'А'),
    e.KEY_G:          ('п', 'П'),
    e.KEY_H:          ('р', 'Р'),
    e.KEY_J:          ('о', 'О'),
    e.KEY_K:          ('л', 'Л'),
    e.KEY_L:          ('д', 'Д'),
    e.KEY_SEMICOLON:  ('ж', 'Ж'),
    e.KEY_APOSTROPHE: ('э', 'Э'),
    e.KEY_Z:          ('я', 'Я'),
    e.KEY_X:          ('ч', 'Ч'),
    e.KEY_C:          ('с', 'С'),
    e.KEY_V:          ('м', 'М'),
    e.KEY_B:          ('и', 'И'),
    e.KEY_N:          ('т', 'Т'),
    e.KEY_M:          ('ь', 'Ь'),
    e.KEY_COMMA:      ('б', 'Б'),
    e.KEY_DOT:        ('ю', 'Ю'),
    e.KEY_SLASH:      ('.', ','),
    e.KEY_GRAVE:      ('ё', 'Ё'),
    e.KEY_1:          ('1', '!'),
    e.KEY_2:          ('2', '"'),
    e.KEY_3:          ('3', '№'),
    e.KEY_4:          ('4', ';'),
    e.KEY_5:          ('5', '%'),
    e.KEY_6:          ('6', ':'),
    e.KEY_7:          ('7', '?'),
    e.KEY_8:          ('8', '*'),
    e.KEY_9:          ('9', '('),
    e.KEY_0:          ('0', ')'),
    e.KEY_MINUS:      ('-', '_'),
    e.KEY_EQUAL:      ('=', '+'),
}

# Ukrainian — based on XKB symbols/ua (standard Ukrainian ЙЦУКЕН variant)
UA: dict[int, tuple[str, str]] = {
    e.KEY_Q:          ('й', 'Й'),
    e.KEY_W:          ('ц', 'Ц'),
    e.KEY_E:          ('у', 'У'),
    e.KEY_R:          ('к', 'К'),
    e.KEY_T:          ('е', 'Е'),
    e.KEY_Y:          ('н', 'Н'),
    e.KEY_U:          ('г', 'Г'),
    e.KEY_I:          ('ш', 'Ш'),
    e.KEY_O:          ('щ', 'Щ'),
    e.KEY_P:          ('з', 'З'),
    e.KEY_LEFTBRACE:  ('х', 'Х'),
    e.KEY_RIGHTBRACE: ('ї', 'Ї'),
    e.KEY_A:          ('ф', 'Ф'),
    e.KEY_S:          ('і', 'І'),
    e.KEY_D:          ('в', 'В'),
    e.KEY_F:          ('а', 'А'),
    e.KEY_G:          ('п', 'П'),
    e.KEY_H:          ('р', 'Р'),
    e.KEY_J:          ('о', 'О'),
    e.KEY_K:          ('л', 'Л'),
    e.KEY_L:          ('д', 'Д'),
    e.KEY_SEMICOLON:  ('ж', 'Ж'),
    e.KEY_APOSTROPHE: ('є', 'Є'),
    e.KEY_Z:          ('я', 'Я'),
    e.KEY_X:          ('ч', 'Ч'),
    e.KEY_C:          ('с', 'С'),
    e.KEY_V:          ('м', 'М'),
    e.KEY_B:          ('и', 'И'),
    e.KEY_N:          ('т', 'Т'),
    e.KEY_M:          ('ь', 'Ь'),
    e.KEY_COMMA:      ('б', 'Б'),
    e.KEY_DOT:        ('ю', 'Ю'),
    e.KEY_SLASH:      ('.', ','),
    e.KEY_GRAVE:      ('ґ', 'Ґ'),
    e.KEY_1:          ('1', '!'),
    e.KEY_2:          ('2', '"'),
    e.KEY_3:          ('3', '№'),
    e.KEY_4:          ('4', ';'),
    e.KEY_5:          ('5', '%'),
    e.KEY_6:          ('6', ':'),
    e.KEY_7:          ('7', '?'),
    e.KEY_8:          ('8', '*'),
    e.KEY_9:          ('9', '('),
    e.KEY_0:          ('0', ')'),
    e.KEY_MINUS:      ('-', '_'),
    e.KEY_EQUAL:      ('=', '+'),
}

# German — based on XKB symbols/de (QWERTZ layout)
DE: dict[int, tuple[str, str]] = {
    e.KEY_Q:          ('q', 'Q'),
    e.KEY_W:          ('w', 'W'),
    e.KEY_E:          ('e', 'E'),
    e.KEY_R:          ('r', 'R'),
    e.KEY_T:          ('t', 'T'),
    e.KEY_Y:          ('z', 'Z'),
    e.KEY_U:          ('u', 'U'),
    e.KEY_I:          ('i', 'I'),
    e.KEY_O:          ('o', 'O'),
    e.KEY_P:          ('p', 'P'),
    e.KEY_LEFTBRACE:  ('ü', 'Ü'),
    e.KEY_RIGHTBRACE: ('+', '*'),
    e.KEY_A:          ('a', 'A'),
    e.KEY_S:          ('s', 'S'),
    e.KEY_D:          ('d', 'D'),
    e.KEY_F:          ('f', 'F'),
    e.KEY_G:          ('g', 'G'),
    e.KEY_H:          ('h', 'H'),
    e.KEY_J:          ('j', 'J'),
    e.KEY_K:          ('k', 'K'),
    e.KEY_L:          ('l', 'L'),
    e.KEY_SEMICOLON:  ('ö', 'Ö'),
    e.KEY_APOSTROPHE: ('ä', 'Ä'),
    e.KEY_Z:          ('y', 'Y'),
    e.KEY_X:          ('x', 'X'),
    e.KEY_C:          ('c', 'C'),
    e.KEY_V:          ('v', 'V'),
    e.KEY_B:          ('b', 'B'),
    e.KEY_N:          ('n', 'N'),
    e.KEY_M:          ('m', 'M'),
    e.KEY_COMMA:      (',', ';'),
    e.KEY_DOT:        ('.', ':'),
    e.KEY_SLASH:      ('#', "'"),
    e.KEY_GRAVE:      ('^', '°'),
    e.KEY_1:          ('1', '!'),
    e.KEY_2:          ('2', '"'),
    e.KEY_3:          ('3', '§'),
    e.KEY_4:          ('4', '$'),
    e.KEY_5:          ('5', '%'),
    e.KEY_6:          ('6', '&'),
    e.KEY_7:          ('7', '/'),
    e.KEY_8:          ('8', '('),
    e.KEY_9:          ('9', ')'),
    e.KEY_0:          ('0', '='),
    e.KEY_MINUS:      ('ß', '?'),
    e.KEY_EQUAL:      ('´', '`'),
}

LAYOUTS: dict[str, dict[int, tuple[str, str]]] = {
    "en": EN,
    "ru": RU,
    "ua": UA,
    "de": DE,
}


def get_layout(name: str) -> dict[int, tuple[str, str]]:
    layout = LAYOUTS.get(name)
    if layout is None:
        raise ValueError(
            f"Unknown layout '{name}'. Available: {sorted(LAYOUTS.keys())}"
        )
    return layout


def keycode_to_char(keycode: int, layout_name: str, shifted: bool = False) -> str:
    layout = LAYOUTS.get(layout_name)
    if layout is None or keycode not in layout:
        return ""
    lo, hi = layout[keycode]
    return hi if shifted else lo


def build_char_map(layout_a: str, layout_b: str) -> dict[str, str]:
    """Build a conversion dict: chars in layout_a → chars in layout_b."""
    a = get_layout(layout_a)
    b = get_layout(layout_b)
    result: dict[str, str] = {}
    for kc in a:
        if kc in b:
            a_lo, a_hi = a[kc]
            b_lo, b_hi = b[kc]
            result[a_lo] = b_lo
            result[a_hi] = b_hi
    return result


def build_reverse_map(layout_name: str) -> dict[str, tuple[int, bool]]:
    """Build a map: char → (keycode, shifted)."""
    layout = get_layout(layout_name)
    result: dict[str, tuple[int, bool]] = {}
    for kc, (lo, hi) in layout.items():
        result[lo] = (kc, False)
        result[hi] = (kc, True)
    return result


_REVERSE_MAPS: dict[str, dict[str, tuple[int, bool]]] = {}


def char_to_keycode(char: str, layout_name: str) -> tuple[int, bool] | None:
    """Find keycode and shift state for a character in a layout."""
    if layout_name not in _REVERSE_MAPS:
        _REVERSE_MAPS[layout_name] = build_reverse_map(layout_name)
    return _REVERSE_MAPS[layout_name].get(char)


# Legacy aliases for backward compatibility
EN_TO_RU = build_char_map("en", "ru")
RU_TO_EN = build_char_map("ru", "en")
