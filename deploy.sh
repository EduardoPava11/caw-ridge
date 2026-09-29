#!/bin/sh
# Build the site and publish it.
#
# The built site is some hundreds of megabytes of images that change with every build,
# so it is kept out of the source history. It goes to the gh-pages branch as a single
# commit that replaces the one before it.
set -e
cd "$(dirname "$0")"
REMOTE=$(git remote get-url origin)

cargo run --release -- "${1:-all}"

cd docs
rm -rf .git
git init -q -b gh-pages
git add -A
git -c commit.gpgsign=false commit -q -m "Site built $(date -u +%Y-%m-%dT%H:%MZ)"
git push -f -q "$REMOTE" gh-pages
rm -rf .git
echo "published"
