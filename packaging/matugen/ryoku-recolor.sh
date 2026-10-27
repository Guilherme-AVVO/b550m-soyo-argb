#!/bin/sh
# post_hook do matugen no Ryoku. Os papéis que o Ryoku passa ao matugen vêm esmaecidos (primary #c2c6d5 num
# wallpaper azulado), então as cores saem do tom 60 das paletas tonais em tones.json: o tom forte de destaque.
# Script POSIX porque o matugen roda o hook no $SHELL do usuário, que pode ser fish.
set -e
tones="${XDG_CACHE_HOME:-$HOME/.cache}/ryoku/tones.json"
color=$(jq -r '.primary."60"' "$tones")
color2=$(jq -r '.tertiary."60"' "$tones")
exec soyo-argb recolor --color "$color" --color2 "$color2"
