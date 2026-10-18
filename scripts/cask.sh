#!/usr/bin/env bash
# Print the Homebrew cask for a release, for the afatmustafa/homebrew-tap repository.
#
#   ./scripts/cask.sh 0.2.0 <sha256 of Swapbar.dmg> > Casks/swapbar.rb
#   ./scripts/cask.sh 0.2.0                          # reads the sha256 from the GitHub release
set -euo pipefail

VERSION="${1:?usage: cask.sh VERSION [SHA256]}"
SHA256="${2:-}"
if [[ -z "$SHA256" ]]; then
  SHA256=$(curl -fsSL "https://github.com/afatmustafa/swapbar/releases/download/v$VERSION/Swapbar.dmg.sha256" | cut -d' ' -f1)
fi
[[ "$SHA256" =~ ^[0-9a-f]{64}$ ]] || { echo "not a sha256: $SHA256" >&2; exit 1; }

cat <<CASK
cask "swapbar" do
  version "$VERSION"
  sha256 "$SHA256"

  url "https://github.com/afatmustafa/swapbar/releases/download/v#{version}/Swapbar.dmg"
  name "Swapbar"
  desc "Menu bar quota monitor and account switcher for claude-swap"
  homepage "https://swapbar.afat.me/"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: :sonoma

  app "Swapbar.app"

  uninstall launchctl: "com.miova.swapbar",
            quit:      "com.miova.swapbar"

  zap trash: [
    "~/.config/swapbar",
    "~/Library/LaunchAgents/com.miova.swapbar.plist",
  ]

  caveats <<~EOS
    Swapbar shows the accounts of claude-swap (cswap). Install it and add your accounts first:
      https://github.com/realiti4/claude-swap
  EOS
end
CASK
