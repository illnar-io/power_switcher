import stat

from power_switcher.core.key_processor import audit_log, configure_audit_log


def test_configure_audit_log_is_private_and_idempotent(tmp_path):
    before = set(audit_log.handlers)
    target = tmp_path / "private" / "audit.log"
    try:
        configure_audit_log(target)
        configure_audit_log(target)
        assert target.exists()
        assert stat.S_IMODE(target.stat().st_mode) == 0o600
        assert stat.S_IMODE(target.parent.stat().st_mode) == 0o700
        handlers = [h for h in audit_log.handlers if getattr(h, "baseFilename", None) == str(target)]
        assert len(handlers) == 1
    finally:
        for handler in set(audit_log.handlers) - before:
            audit_log.removeHandler(handler)
            handler.close()
