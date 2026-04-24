class Medium < Formula
  desc "Personal service-access overlay CLI"
  homepage "https://example.invalid/medium"
  url "https://example.invalid/medium-0.1.0.tar.gz"
  sha256 "REPLACE_WITH_ARCHIVE_SHA256"
  license "MIT"
  version "0.1.0"

  def install
    bin.install "medium"
  end

  test do
    assert_match "usage: medium", shell_output("#{bin}/medium 2>&1", 1)
  end
end
