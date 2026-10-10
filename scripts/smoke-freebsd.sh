#!/bin/sh
# Run inside the matching FreeBSD VM after the host packages the release archive.
set -eu

target=$1
version=$2
test "$(uname -s)" = FreeBSD
directory=$(mktemp -d)
trap 'rm -rf "$directory"' 0
tar -xzf "dist/renrs-v$version-$target.tar.gz" -C "$directory"
binaries="$directory/renrs-v$version-$target"

for name in ren renx unren; do
  test "$("$binaries/$name" --version)" = "$name $version"
  "$binaries/$name" --help
done

mkdir "$directory/work"
cd "$directory/work"
printf 'original contents\n' > expected
cp expected old.txt
"$binaries/ren" old.txt new.txt
test ! -e old.txt
cmp expected new.txt
"$binaries/unren" old.txt new.txt
test ! -e new.txt
cmp expected old.txt
"$binaries/renx" --append .bak old.txt
test ! -e old.txt
cmp expected old.txt.bak

printf 'distinct source\n' > source-expected
cp source-expected old.txt
if "$binaries/ren" old.txt old.txt.bak; then
  echo 'ren unexpectedly overwrote an existing destination' >&2
  exit 1
fi
cmp source-expected old.txt
cmp expected old.txt.bak
echo "FreeBSD $target archive smoke checks passed"
