#!/bin/sh
version="1.1"
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[1;34m'
DARK='\033[1;30m'
NC='\033[0m'

echo "${BLUE}rslocal binary installer ${version}${NC}"
unameOut="$(uname -s)"

case "${unameOut}" in
Darwin*)
  arch=macos-x86_64
  ;;
*)
  arch=linux-x86_64
  ;;
esac
bin_dir="/usr/local/bin"
url=$(curl -s https://api.github.com/repos/saltbo/rslocal/releases/latest | grep "browser_download_url.*${arch}.tar.gz\"" | cut -d : -f 2,3 | tr -d '\"[:space:]')

echo "${DARK}"
echo "Configuration: [${arch}]"
echo "Location:      [${url}]"
echo "Directory:     [${bin_dir}]"
echo "${NC}"

test ! -d "${bin_dir}" && mkdir "${bin_dir}"
curl -J -L "${url}" | tar xz -C "${bin_dir}"

if [ $? -eq 0 ]; then
  echo "${GREEN}"
  echo "Installation completed successfully."
  echo "$ rslocal version"
  "${bin_dir}"/rslocal version
else
  echo "${RED}"
  echo "Failed installing rslocal"
fi

echo "${NC}"
