"""cl.exe wrapper that strips GCC-only flags incompatible with MSVC."""
import sys
import subprocess
import os

GCC_ONLY_PREFIXES = ("-Wno-", "-Wstrict-", "-Wextra", "-Wpedantic", "-Wshadow")

args = [
    a for a in sys.argv[1:]
    if not any(a.startswith(p) for p in GCC_ONLY_PREFIXES)
]

cl = os.environ.get("CL_REAL", "cl.exe")
sys.exit(subprocess.call([cl] + args))
