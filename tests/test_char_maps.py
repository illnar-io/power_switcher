import pytest

from power_switcher.maps.keycode_map import (
    EN_TO_RU,
    RU_TO_EN,
    build_char_map,
    char_to_keycode,
    LAYOUTS,
    keycode_to_char,
)
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
    assert "fr" in LAYOUTS
    assert "es" in LAYOUTS
    assert "it" in LAYOUTS
    assert "pl" in LAYOUTS


def test_keycode_to_char_fr_azerty():
    # AZERTY swaps A/Q, Z/W and puts M on the home row.
    assert keycode_to_char(e.KEY_Q, "fr") == "a"
    assert keycode_to_char(e.KEY_A, "fr") == "q"
    assert keycode_to_char(e.KEY_W, "fr") == "z"
    assert keycode_to_char(e.KEY_Z, "fr") == "w"
    assert keycode_to_char(e.KEY_SEMICOLON, "fr") == "m"
    assert keycode_to_char(e.KEY_2, "fr", shifted=True) == "2"
    assert keycode_to_char(e.KEY_2, "fr") == "é"


def test_keycode_to_char_es():
    assert keycode_to_char(e.KEY_SEMICOLON, "es") == "ñ"
    assert keycode_to_char(e.KEY_SEMICOLON, "es", shifted=True) == "Ñ"
    assert keycode_to_char(e.KEY_LEFTBRACE, "es") == "`"
    assert keycode_to_char(e.KEY_A, "es") == "a"


def test_keycode_to_char_it():
    assert keycode_to_char(e.KEY_SEMICOLON, "it") == "ò"
    assert keycode_to_char(e.KEY_APOSTROPHE, "it") == "à"
    assert keycode_to_char(e.KEY_A, "it") == "a"


def test_keycode_to_char_de_backslash_and_slash():
    # Regression: German '\' is on the '#' key and '-' stays on the '-' key.
    assert keycode_to_char(e.KEY_BACKSLASH, "de") == "#"
    assert keycode_to_char(e.KEY_SLASH, "de") == "-"
    assert keycode_to_char(e.KEY_SLASH, "de", shifted=True) == "_"


@pytest.mark.parametrize("layout", sorted(LAYOUTS))
def test_every_layout_char_round_trips(layout):
    """Every printable character must map back to the key that produced it."""
    for keycode, (lo, hi) in LAYOUTS[layout].items():
        assert char_to_keycode(lo, layout) == (keycode, False)
        assert char_to_keycode(hi, layout) == (keycode, True)


@pytest.mark.parametrize("layout", ["en", "de", "fr", "es", "it", "pl"])
def test_latin_layouts_type_all_ascii_letters(layout):
    for ch in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ":
        assert char_to_keycode(ch, layout) is not None, f"{layout} cannot type {ch!r}"


def test_en_de_conversion_yz():
    en_to_de = build_char_map("en", "de")
    de_to_en = build_char_map("de", "en")
    assert en_to_de["y"] == "z"
    assert de_to_en["z"] == "y"
