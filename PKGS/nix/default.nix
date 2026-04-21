{ pkgs ? import <nixpkgs> {} }:

let
  version = "2.7.0";
  
  archMap = {
    "x86_64-linux" = {
      arch = "amd64";
      hash = "197d07a1ca9811f835e23423c5527db5b0015fde4088f6d40695c93eee498fef";
    };
    "aarch64-linux" = {
      arch = "arm64";
      hash = "f4447ab36132827869347945b2575c855a91e9fd0ba65b6c0115341382868a2c";
    };
    "armv6l-linux" = {
      arch = "armhfv6";
      hash = "904e4201edfc285546a97e8aeea544168c78b4b2677c673808e1919b802e17a9";
    };
    "armv7l-linux" = {
      arch = "armhfv7";
      hash = "a31a4ad4713910b3d00168cc158dce72eb4dd9b12a549758da87ad71cd0acda8";
    };
    "riscv64-linux" = {
      arch = "riscv64";
      hash = "d3c06798bf373918fd88b037349e99d9f44dd513bdf3de21c0e20e1b7ea5d8fe";
    };
  };
  
  info = archMap.${pkgs.stdenv.hostPlatform.system} or (throw "Unsupported system: ${pkgs.stdenv.hostPlatform.system}");
  
  src = pkgs.fetchurl {
    url = "https://github.com/ljufa/rsplayer/releases/download/${version}/rsplayer_${info.arch}";
    sha256 = info.hash;
  };

in pkgs.stdenv.mkDerivation {
  pname = "rsplayer";
  inherit version;
  
  inherit src;
  dontUnpack = true;
  
  installPhase = ''
    mkdir -p $out/bin
    cp $src $out/bin/rsplayer
    chmod 755 $out/bin/rsplayer
  '';
  
  meta = with pkgs.lib; {
    description = "RSPlayer - Music Player";
    homepage = "https://github.com/ljufa/rsplayer";
    license = licenses.mit;
    platforms = builtins.attrNames archMap;
    mainProgram = "rsplayer";
  };
}
