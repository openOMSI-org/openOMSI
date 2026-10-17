#!/bin/sh
# The openOMSI dedicated server, asked for what it needs: the first time the OMSI 2 folder
# and the main settings of server.cfg, every time the map (a numbered list of the maps
# installed); and, if wanted, a start-<map>.sh beside it that starts that map again without
# a question. The server's output goes to this window and to server.log.
#
#   run-server.sh              ask for the map (and the rest the first time)
#   run-server.sh --setup      ask for the folder and the settings again
#   run-server.sh --map maps/Grundorf/global.cfg   no question: this map
#
# (start.sh stays as it was: start.sh /path/to/OMSI2 [server.cfg])
set -u
here="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
exe="$here/openomsi"
conf="$here/run-server.conf"
cfg="$here/server.cfg"
log="$here/server.log"

case "${LC_ALL:-${LC_MESSAGES:-${LANG:-}}}" in
  pt*) pt=1 ;;
  *) pt=0 ;;
esac
# say "english" "português"
say() { if [ "$pt" = 1 ]; then printf '%s\n' "$2"; else printf '%s\n' "$1"; fi; }
# (a question goes to stderr: it is seen when the answer is taken by $(...))
ask() { if [ "$pt" = 1 ]; then printf '%s' "$2" >&2; else printf '%s' "$1" >&2; fi; }
die() { say "$1" "$2" >&2; exit 1; }

setup=0
map=""
while [ $# -gt 0 ]; do
  case "$1" in
    --setup) setup=1 ;;
    --map) [ $# -ge 2 ] || die "--map needs a map (maps/<folder>/global.cfg)" "--map precisa de um mapa (maps/<pasta>/global.cfg)"; map="$2"; shift ;;
    -h|--help) sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option: $1 (see --help)" "opção desconhecida: $1 (veja --help)" ;;
  esac
  shift
done

[ -x "$exe" ] || die "openomsi is not beside this script ($exe)" "o openomsi não está ao lado deste script ($exe)"

# run-server.conf: root=<OMSI 2 folder>, last=<last map picked>
conf_get() { [ -f "$conf" ] && sed -n "s/^$1=//p" "$conf" | tail -n 1; }
conf_set() {
  tmp="$conf.tmp.$$"
  { [ -f "$conf" ] && grep -v "^$1=" "$conf"; printf '%s=%s\n' "$1" "$2"; } > "$tmp" && mv "$tmp" "$conf"
}

# server.cfg: the value of a key, and a key set (its line replaced, or added at the end)
cfg_get() { [ -f "$cfg" ] && awk -F= -v k="$1" '{ key = $1; gsub(/^[ \t]+|[ \t]+$/, "", key) } key == k && $0 !~ /^[ \t]*#/ { v = substr($0, index($0, "=") + 1); gsub(/^[ \t]+|[ \t]+$/, "", v); last = v } END { print last }' "$cfg"; }
cfg_set() {
  tmp="$cfg.tmp.$$"
  K="$1" V="$2" awk '
    BEGIN { k = ENVIRON["K"]; v = ENVIRON["V"]; done = 0 }
    { key = $0; sub(/=.*/, "", key); gsub(/^[ \t]+|[ \t]+$/, "", key) }
    $0 !~ /^[ \t]*#/ && index($0, "=") && key == k { if (!done) print k " = " v; done = 1; next }
    { print }
    END { if (!done) print k " = " v }' "$cfg" > "$tmp" && mv "$tmp" "$cfg"
}

maps_file="$(mktemp)"
err_file="$(mktemp)"
status_file="$(mktemp)"
# (the terminal's echo comes back however the script ends: a Ctrl+C at the password left it off)
trap 'rm -f "$maps_file" "$err_file" "$status_file"; if [ -t 0 ]; then stty echo 2>/dev/null; fi' EXIT
# (Ctrl+C during the questions ends the script through its EXIT trap, in every sh)
trap 'exit 130' INT

# the installed maps of a folder (and server.cfg written with its defaults when missing);
# false with the reason shown when the folder is not a complete OMSI 2
list_maps() {
  if "$exe" --root "$1" --server "$cfg" --list-maps > "$maps_file" 2> "$err_file"; then
    return 0
  fi
  grep -v ' INFO ' "$err_file" | grep -v '^[[:space:]]*$' >&2
  return 1
}

root="$(conf_get root)"
first=0
[ -n "$root" ] && [ ! -f "$cfg" ] && first=1
if [ -z "$root" ] || [ "$setup" = 1 ]; then
  first=1
  while :; do
    ask "Folder of the original OMSI 2 (the one with Omsi.exe, maps and Vehicles): " "Pasta do OMSI 2 original (a que tem Omsi.exe, maps e Vehicles): "
    IFS= read -r answer || exit 1
    # (a path dragged into a terminal comes in quotes)
    answer="$(printf '%s' "$answer" | sed "s/^[[:space:]]*['\"]\{0,1\}//; s/['\"]\{0,1\}[[:space:]]*$//")"
    [ -n "$answer" ] || continue
    if list_maps "$answer"; then
      root="$answer"
      conf_set root "$root"
      break
    fi
    say "That folder is not a complete OMSI 2 (see above). Try again, or Ctrl+C." "Essa pasta não é um OMSI 2 completo (veja acima). Tente de novo, ou Ctrl+C."
  done
else
  list_maps "$root" || die "The OMSI 2 folder saved in run-server.conf no longer works: run-server.sh --setup" "A pasta do OMSI 2 salva em run-server.conf não funciona mais: run-server.sh --setup"
fi

# the main settings, the first time (Enter keeps what server.cfg has)
setting() {
  key="$1"; now="$(cfg_get "$key")"
  ask "$2 [$now]: " "$3 [$now]: "
  IFS= read -r v || exit 1
  printf '%s' "$v"
}
number() {
  while :; do
    v="$(setting "$1" "$2" "$3")"
    [ -z "$v" ] && return 0
    case "$v" in *[!0-9]*) ;; *) if [ "$v" -ge "$4" ] && [ "$v" -le "$5" ]; then cfg_set "$1" "$v"; return 0; fi ;; esac
    say "A whole number from $4 to $5." "Um número inteiro de $4 a $5."
  done
}
if [ "$first" = 1 ]; then
  say "" ""
  say "Server settings (Enter keeps the value in brackets; server.cfg has the others):" "Configurações do servidor (Enter mantém o valor entre colchetes; o server.cfg tem as outras):"
  v="$(setting name "Server name" "Nome do servidor")"
  [ -n "$v" ] && cfg_set name "$v"
  number max_players "Players at most (1-64)" "Máximo de jogadores (1-64)" 1 64
  number port "UDP port of the game" "Porta UDP do jogo" 1 65535
  number web_port "Web port (status page, players by WebSocket)" "Porta web (página de status, jogadores por WebSocket)" 1 65535
  while :; do
    ask "Public address through a Cloudflare tunnel (no port forwarding needed)? [y/n, now $(cfg_get tunnel)]: " "Endereço público por túnel da Cloudflare (sem abrir porta no roteador)? [s/n, agora $(cfg_get tunnel)]: "
    IFS= read -r v || exit 1
    case "$v" in
      "") break ;;
      [yYsS]*) cfg_set tunnel 1; break ;;
      [nN]*) cfg_set tunnel 0; break ;;
    esac
  done
  while :; do
    if [ -n "$(cfg_get admin_password)" ]; then
      ask "Admin password (12+ characters; Enter keeps the one set; - for none): " "Senha de admin (12+ caracteres; Enter mantém a atual; - para nenhuma): "
    else
      ask "Admin password (12+ characters; Enter for none): " "Senha de admin (12+ caracteres; Enter para nenhuma): "
    fi
    if [ -t 0 ]; then stty -echo; fi
    IFS= read -r v || v=""
    if [ -t 0 ]; then stty echo; fi
    echo
    # (one kept from before the 12 characters: the server would ignore it)
    if [ -z "$v" ] && [ -n "$(cfg_get admin_password)" ] && [ "$(cfg_get admin_password | tr -d '\n' | wc -m)" -lt 12 ]; then
      say "The password set is shorter than 12 characters, and the server ignores it: type a new one (- for none)." "A senha atual tem menos de 12 caracteres, e o servidor a ignora: digite uma nova (- para nenhuma)."
      # (answers from a file: said, not asked again for ever)
      if [ -t 0 ]; then continue; fi
    fi
    case "$v" in
      "") break ;;
      -) cfg_set admin_password ""; break ;;
    esac
    if [ "$(printf '%s' "$v" | wc -m)" -lt 12 ]; then
      say "At least 12 characters." "Pelo menos 12 caracteres."
    elif [ "$v" != "$(printf '%s' "$v" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')" ]; then
      say "No spaces at the start or the end (server.cfg would cut them)." "Sem espaço no começo ou no fim (o server.cfg cortaria)."
    else
      cfg_set admin_password "$v"
      break
    fi
  done
  say "Saved in $cfg." "Salvo em $cfg."
fi

# the map: given, or picked from the list
[ -s "$maps_file" ] || die "No map is installed in $root/maps." "Nenhum mapa instalado em $root/maps."
if [ -n "$map" ]; then
  grep -Fxq -- "$map" "$maps_file" || {
    say "The map $map is not installed. Installed:" "O mapa $map não está instalado. Instalados:" >&2
    sed 's/^/  /' "$maps_file" >&2
    exit 2
  }
  ask_shortcut=0
else
  last="$(conf_get last)"
  [ -n "$last" ] || last="$(cfg_get map)"
  say "" ""
  say "Maps:" "Mapas:"
  n=0; dflt=""
  while IFS= read -r m; do
    n=$((n + 1))
    mark=""
    [ "$m" = "$last" ] && { dflt=$n; mark=" *"; }
    name="${m#maps/}"; name="${name%/global.cfg}"
    printf '  %3d  %s%s\n' "$n" "$name" "$mark"
  done < "$maps_file"
  while :; do
    if [ -n "$dflt" ]; then ask "Map number [Enter: $dflt]: " "Número do mapa [Enter: $dflt]: "; else ask "Map number: " "Número do mapa: "; fi
    IFS= read -r v || exit 1
    [ -z "$v" ] && v="$dflt"
    case "$v" in ""|*[!0-9]*) continue ;; esac
    if [ "$v" -ge 1 ] && [ "$v" -le "$n" ]; then
      map="$(sed -n "${v}p" "$maps_file")"
      break
    fi
  done
  ask_shortcut=1
fi
conf_set last "$map"

# a shortcut that starts this map again with no question (only when asked)
if [ "$ask_shortcut" = 1 ]; then
  name="${map#maps/}"; name="${name%/global.cfg}"
  # (a file name a terminal and a file manager both take: the common accents dropped)
  slug="$(printf '%s' "$name" | sed -e 's/á/a/g' -e 's/à/a/g' -e 's/â/a/g' -e 's/ã/a/g' -e 's/ä/a/g' -e 's/å/a/g' -e 's/Á/A/g' -e 's/À/A/g' -e 's/Â/A/g' -e 's/Ã/A/g' -e 's/Ä/A/g' -e 's/Å/A/g' -e 's/é/e/g' -e 's/è/e/g' -e 's/ê/e/g' -e 's/ë/e/g' -e 's/É/E/g' -e 's/È/E/g' -e 's/Ê/E/g' -e 's/Ë/E/g' -e 's/í/i/g' -e 's/ì/i/g' -e 's/î/i/g' -e 's/ï/i/g' -e 's/Í/I/g' -e 's/Ì/I/g' -e 's/Î/I/g' -e 's/Ï/I/g' -e 's/ó/o/g' -e 's/ò/o/g' -e 's/ô/o/g' -e 's/õ/o/g' -e 's/ö/o/g' -e 's/Ó/O/g' -e 's/Ò/O/g' -e 's/Ô/O/g' -e 's/Õ/O/g' -e 's/Ö/O/g' -e 's/ú/u/g' -e 's/ù/u/g' -e 's/û/u/g' -e 's/ü/u/g' -e 's/Ú/U/g' -e 's/Ù/U/g' -e 's/Û/U/g' -e 's/Ü/U/g' -e 's/ç/c/g' -e 's/Ç/C/g' -e 's/ñ/n/g' -e 's/Ñ/N/g' -e 's/ß/ss/g')"
  slug="$(printf '%s' "$slug" | tr -c 'A-Za-z0-9._-' '-' | tr -s '-' | sed 's/^-//; s/-$//')"
  [ -n "$slug" ] || slug="map"
  shortcut="$here/start-$slug.sh"
  if [ ! -e "$shortcut" ]; then
    ask "Create start-$slug.sh, which starts this map again without asking? [y/N]: " "Criar start-$slug.sh, que inicia este mapa de novo sem perguntar? [s/N]: "
    IFS= read -r v || v=""
    case "$v" in
      [yYsS]*)
        quoted="$(printf '%s' "$map" | sed "s/'/'\\\\''/g")"
        printf '#!/bin/sh\n# openOMSI server on %s, with run-server.sh (made by it)\nexec "$(dirname -- "$0")/run-server.sh" --map '"'"'%s'"'"' "$@"\n' "$name" "$quoted" > "$shortcut"
        chmod +x "$shortcut"
        say "Made $shortcut." "Criado $shortcut."
        ;;
    esac
  fi
fi

say "" ""
say "Starting on $map (the log also goes to server.log; Ctrl+C stops the server)." "Iniciando em $map (o log também vai para server.log; Ctrl+C para o servidor)."
# (Ctrl+C is the server's: it stops cleanly and says so through tee, which -i keeps alive,
# and this script tells what became of it)
trap ':' INT
{ "$exe" --root "$root" --server "$cfg" --server-map "$map" 2>&1; echo $? > "$status_file"; } | tee -i -a "$log"
code="$(cat "$status_file" 2>/dev/null)"
case "$code" in
  0|130|143|"") ;;
  1) say "The server stopped (code 1): the OMSI 2 folder is incomplete or wrong, or another error - see the lines above. run-server.sh --setup chooses the folder again." "O servidor parou (código 1): a pasta do OMSI 2 está incompleta ou errada, ou outro erro - veja as linhas acima. run-server.sh --setup escolhe a pasta de novo." ;;
  2) say "The server stopped (code 2): server.cfg cannot be read, or the map is not installed - see the lines above." "O servidor parou (código 2): o server.cfg não pôde ser lido, ou o mapa não está instalado - veja as linhas acima." ;;
  3) say "The server stopped (code 3): its UDP port is taken (another server?). Stop it, or choose another port: run-server.sh --setup." "O servidor parou (código 3): a porta UDP está ocupada (outro servidor?). Feche-o, ou escolha outra porta: run-server.sh --setup." ;;
  *) say "The server stopped (code $code) - see the lines above and server.log." "O servidor parou (código $code) - veja as linhas acima e o server.log." ;;
esac
exit "${code:-0}"
