{ naersk, src, lib, cmake, pkg-config, postgresql_14 }:

naersk.buildPackage {
  pname = "lofi";
  version = "0.1.0";

  src = ./.;

  cargoSha256 = lib.fakeSha256;

  buildInputs = [ cmake postgresql_14 ];
  nativeBuildInputs = [ pkg-config ];

  meta = with lib; {
    description = "Tool to correlate the r09 telegrams to transmission locations";
    homepage = "https://github.com/dump-dvb/lofi";
  };
}
