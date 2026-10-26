#!/bin/sh
# Fixed protocol failures for the native coordinator transport tests.
set -eu

if [ "$#" -ne 3 ] || [ "$1" != debate-session ] || [ "$2" != --run ]; then
  exit 2
fi
IFS= read -r start
case "${3##*/}" in
  eof)
    exit 0
    ;;
  partial)
    printf '%s' '{"kind":'
    ;;
  terminal)
    printf '%s\n' '{"kind":"completed","rounds":0,"calls":0}'
    ;;
  dispatch)
    printf '%s\n' '{"kind":"dispatch","dispatch":{"round":0,"calls":[]}}'
    ;;
  failed)
    printf '%s\n' '{"kind":"failed","message":"confidential payload"}'
    ;;
  *)
    exit 2
    ;;
esac
