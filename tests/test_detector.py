from pathlib import Path

from power_switcher.maps.layout_set import LayoutSet

PROFILES_DIR = Path(__file__).parents[1] / "power_switcher" / "detection" / "profiles"


def _make_set(layouts: list[str] = None) -> LayoutSet:
    if layouts is None:
        layouts = ["en", "ru"]
    return LayoutSet.create(layouts, PROFILES_DIR)


def test_en_ru_ghbdtn():
    ls = _make_set()
    assert ls.find_best_layout("ghbdtn", "en") == "ru"


def test_en_ru_hello():
    ls = _make_set()
    assert ls.find_best_layout("hello", "en") is None


def test_ru_en_privet():
    ls = _make_set()
    assert ls.find_best_layout("привет", "ru") is None


def test_short_word_skipped():
    ls = _make_set()
    assert ls.find_best_layout("ab", "en") is None


def test_digit_word_skipped():
    ls = _make_set()
    assert ls.find_best_layout("abc123", "en") is None


# ── punctuation keys that are letters in another layout ─────────────────


def test_punctuated_word_comma_key_converts():
    # "работа" typed in the English layout: the comma key is 'б'
    # (р=KEY_H, а=KEY_F, б=KEY_COMMA, о=KEY_J, т=KEY_N, а=KEY_F).
    ls = _make_set()
    assert ls.find_best_layout("hf,jnf", "en") == "ru"


def test_punctuated_word_dot_key_converts():
    # "люди" typed in English: the dot key is 'ю' → "k.lb".
    ls = _make_set()
    assert ls.find_best_layout("k.lb", "en") == "ru"


def test_punctuated_word_semicolon_key_converts():
    # "тоже" typed in English: the semicolon key is 'ж' → "nj;t".
    ls = _make_set()
    assert ls.find_best_layout("nj;t", "en") == "ru"


def test_apostrophe_words_are_left_alone():
    # Genuine English with punctuation must never be converted, even though the
    # apostrophe key is 'э' in Russian.
    ls = _make_set()
    assert ls.find_best_layout("don't", "en") is None
    assert ls.find_best_layout("it's", "en") is None


def test_dotted_email_not_converted():
    ls = _make_set()
    assert ls.find_best_layout("user@example.com", "en") is None


def test_convert_en_to_ru():
    ls = _make_set()
    assert ls.convert("ghbdtn", "en", "ru") == "привет"


def test_convert_ru_to_en():
    ls = _make_set()
    assert ls.convert("привет", "ru", "en") == "ghbdtn"


def test_multi_layout_en_ru_ua():
    ls = _make_set(["en", "ru", "ua"])
    # "ghbdtn" in RU is "привет"
    # "ghbdtn" in UA is "привет" (same bigrams mostly)
    # But let's try something specific to UA: "і", "є", "ї"
    # "s" (EN) -> "ы" (RU)
    # "s" (EN) -> "і" (UA)
    assert ls.convert("s", "en", "ua") == "і"
    assert ls.convert("s", "en", "ru") == "ы"


def test_keycode_to_char_via_set():
    ls = _make_set()
    from evdev import ecodes as e
    assert ls.keycode_to_char(e.KEY_G, "en", shifted=False) == "g"
    assert ls.keycode_to_char(e.KEY_G, "ru", shifted=False) == "п"
