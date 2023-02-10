{ craneLib, src, lib, cmake, pkg-config, postgresql_14 }:

craneLib.buildPackage {
  pname = "lofi";
  version = "0.1.0";

  src = ./.;
  cargoExtraArgs = "--bin lofi --features=build-binary";

  buildInputs = [ cmake postgresql_14 ];
  nativeBuildInputs = [ pkg-config ];

  meta = {
    description = "Tool to correlate the r09 telegrams to transmission locations";
    homepage = "https://github.com/dump-dvb/lofi";
  };
}
