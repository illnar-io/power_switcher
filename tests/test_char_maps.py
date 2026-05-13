from power_switcher.maps.keycode_map import EN_TO_RU, RU_TO_EN, build_char_map, LAYOUTS, keycode_to_char
from evdev import ecodes as e


def test_en_to_ru_basic():
    assert EN_TO_RU["g"] == "п"
    assert EN_TO_RU["h"] == "р"


def test_en_to_ru_ghbdtn():
    word = "ghbdtn"
    converted = "".join(EN_TO_RU.get(c, c) for c in word)
    assert converted == "привет"


def test_ru_to_en_privet():
    word = "привет"
    converted = "".join(RU_TO_EN.get(c, c) for c in word)
    assert converted == "ghbdtn"


def test_en_to_ru_uppercase():
    assert EN_TO_RU["G"] == "П"
    assert EN_TO_RU["H"] == "Р"


def test_ru_to_en_uppercase():
    assert RU_TO_EN["П"] == "G"
    assert RU_TO_EN["Р"] == "H"


def test_maps_are_inverse():
    for en_char, ru_char in EN_TO_RU.items():
        assert RU_TO_EN.get(ru_char) == en_char, f"RU_TO_EN[{ru_char!r}] != {en_char!r}"


def test_build_char_map_en_de():
    en_to_de = build_char_map("en", "de")
    assert en_to_de["y"] == "z"
    assert en_to_de["z"] == "y"
    assert en_to_de["Y"] == "Z"
    assert en_to_de["Z"] == "Y"


def test_build_char_map_en_ua():
    en_to_ua = build_char_map("en", "ua")
    assert en_to_ua["s"] == "і"
    assert en_to_ua["'"] == "є"
    assert en_to_ua["`" ] == "ґ"


def test_keycode_to_char_en():
    assert keycode_to_char(e.KEY_G, "en", shifted=False) == "g"
    assert keycode_to_char(e.KEY_G, "en", shifted=True) == "G"


def test_keycode_to_char_ru():
    assert keycode_to_char(e.KEY_G, "ru", shifted=False) == "п"
    assert keycode_to_char(e.KEY_G, "ru", shifted=True) == "П"


def test_keycode_to_char_de():
    assert keycode_to_char(e.KEY_Y, "de", shifted=False) == "z"
    assert keycode_to_char(e.KEY_Z, "de", shifted=False) == "y"


def test_keycode_to_char_unknown_layout():
    assert keycode_to_char(e.KEY_G, "xx", shifted=False) == ""


def test_available_layouts():
    assert "en" in LAYOUTS
    assert "ru" in LAYOUTS
    assert "ua" in LAYOUTS
    assert "de" in LAYOUTS
