class Medium < Formula
  desc "Personal service-access overlay CLI"
  homepage "https://github.com/burniq/medium"
  url "https://github.com/burniq/medium/releases/download/v0.0.1/medium-0.0.1.tar.gz"
  sha256 "REPLACE_WITH_ARCHIVE_SHA256"
  license "MIT"
  version "0.0.1"

  def install
    bin.install "bin/medium"
  end

  test do
    assert_match "usage: medium", shell_output("#{bin}/medium 2>&1", 1)
  end
end
