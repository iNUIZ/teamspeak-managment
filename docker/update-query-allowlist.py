"""Trust only the installed Manager's RFC1918 Docker IPv4 for Query flood exemption.

Run in a networkless helper with the selected TS data volume mounted at /target.
Preserves existing entries/ownership, refuses symlinks and explicit deny rules.
"""
import ipaddress
import os
import re
from pathlib import Path
import stat
import sys
import tempfile


def update(root: Path, address: str) -> bool:
    ip = ipaddress.IPv4Address(address)
    if not any(ip in ipaddress.ip_network(net) for net in ("10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16")):
        raise ValueError("Manager address must be a private Docker IPv4")
    target = root / "query_ip_allowlist.txt"
    if target.is_symlink() or not target.is_file():
        raise ValueError("Expected a regular TeamSpeak Query allowlist")
    original_stat = target.stat()
    content = target.read_text(encoding="utf-8")
    begin = "# BEGIN TEAMHUB MANAGER QUERY"
    end = "# END TEAMHUB MANAGER QUERY"
    if (begin in content) != (end in content):
        raise ValueError("Incomplete managed Query block; existing rules were preserved")
    base = re.sub(r"(?ms)^# BEGIN TEAMHUB MANAGER QUERY\n.*?^# END TEAMHUB MANAGER QUERY\n?", "", content)
    backup = root / "query_ip_allowlist.manager-backup"
    # Migrate only the exact output of the earlier append-only helper. This
    # gives future address rotations an owned block, without deleting operator rules.
    if begin not in content and backup.is_file() and not backup.is_symlink():
        previous = backup.read_text(encoding="utf-8")
        prefix = previous + ("\n" if previous and not previous.endswith("\n") else "")
        if content == prefix + f"{ip}/32\n":
            base = previous

    def networks(text):
        for line in text.splitlines():
            item = line.split("#", 1)[0].strip()
            if item:
                try:
                    yield ipaddress.ip_network(item, strict=False)
                except ValueError:
                    pass  # Preserve unknown existing syntax without widening it.

    already_trusted = any(ip in net for net in networks(base) if net.version == 4)
    desired = base + ("\n" if base and not base.endswith("\n") else "")
    if not already_trusted:
        desired += f"{begin}\n{ip}/32\n{end}\n"
    if desired == content:
        return False
    deny = root / "query_ip_denylist.txt"
    if deny.is_symlink():
        raise ValueError("Refusing a symlinked Query denylist")
    if deny.is_file() and any(ip in net for net in networks(deny.read_text(encoding="utf-8")) if net.version == 4):
        raise ValueError("Manager IP is explicitly denied; existing rule was preserved")
    try:
        with backup.open("x", encoding="utf-8") as stream:
            stream.write(content)
        os.chmod(backup, stat.S_IMODE(original_stat.st_mode))
        if hasattr(os, "chown"):
            os.chown(backup, original_stat.st_uid, original_stat.st_gid)
    except FileExistsError:
        pass
    staged = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=root, prefix=".manager-query-", delete=False) as stream:
            staged = Path(stream.name)
            stream.write(desired)
            stream.flush()
            os.fsync(stream.fileno())
        os.chmod(staged, stat.S_IMODE(original_stat.st_mode))
        if hasattr(os, "chown"):
            os.chown(staged, original_stat.st_uid, original_stat.st_gid)
        os.replace(staged, target)
        return True
    finally:
        if staged and staged.exists():
            staged.unlink()


if __name__ == "__main__":
    print("updated" if update(Path("/target"), sys.argv[1]) else "unchanged")
