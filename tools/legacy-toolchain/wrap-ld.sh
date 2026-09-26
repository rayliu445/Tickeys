#!/bin/bash
# Linker wrapper for building Tickeys with the legacy rustc toolchain (see BUILD.md).
#
# Why this exists
# ---------------
# rustc 1.19's rlib archives contain members that are not Mach-O objects:
#   - rust.metadata.bin   (crate metadata, only needed while *compiling*)
#   - *.rcgu.bc.z         (compressed LLVM bitcode, only needed for LTO)
# The ld that ships with macOS 15 refuses to link such an archive:
#   ld: archive member 'rust.metadata.bin' not a mach-o file in '.../libstd-....rlib'
# and -Wl,-ld_classic does not help.
#
# Linking only needs the real object files, and the metadata has already been read
# during compilation, so this wrapper rebuilds each rlib into an archive holding
# nothing but its .o members and hands that to the real linker.
#
# Usage:
#   export RUSTFLAGS="-C linker=/path/to/wrap-ld.sh"
#   # optional: export STRIP_CACHE=/some/writable/dir

set -u

CC=${REAL_CC:-/usr/bin/cc}
STRIP_CACHE=${STRIP_CACHE:-${TMPDIR:-/tmp}/tickeys-stripped-rlibs}
mkdir -p "$STRIP_CACHE" || exit 1

args=()
for a in "$@"; do
	case "$a" in
		*.rlib)
			b=$(basename "$a")
			if [ ! -f "$STRIP_CACHE/$b" ]; then
				t=$(mktemp -d) || exit 1
				if ( cd "$t" && ar x "$a" >/dev/null 2>&1 ) && ls "$t"/*.o >/dev/null 2>&1; then
					( cd "$t" && ar rcs "$STRIP_CACHE/$b" ./*.o >/dev/null 2>&1 )
					ranlib "$STRIP_CACHE/$b" >/dev/null 2>&1
				else
					# no object members (e.g. a metadata-only rlib): pass it through
					cp "$a" "$STRIP_CACHE/$b"
				fi
				rm -rf "$t"
			fi
			args+=("$STRIP_CACHE/$b")
			;;
		*)
			args+=("$a")
			;;
	esac
done

exec "$CC" "${args[@]}"
