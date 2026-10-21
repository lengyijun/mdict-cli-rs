complete -c mdict-cli-rs -f
complete -c mdict-cli-rs -n '__fish_use_subcommand' -l help      -d 'Print the help message.'
complete -c mdict-cli-rs -n '__fish_use_subcommand' -l version   -d 'Print the version'
complete -c mdict-cli-rs -n '__fish_use_subcommand' -l list-dicts -d 'List dictionaries'
complete -c mdict-cli-rs -n '__fish_use_subcommand' -l show-path -d 'Show paths'
complete -c mdict-cli-rs -n '__fish_use_subcommand' -a anki      -d 'anki-like review mode'

set -l anki_folder_cmd '__fish_seen_subcommand_from anki'

complete -c mdict-cli-rs -n $anki_folder_cmd -a all -d 'all favorites words'
complete -c mdict-cli-rs -n $anki_folder_cmd -a '(cat ~/.config/goldendict/favorites | grep folder | grep expanded | grep -oP "(?<=name=\")[^\"]+")' -d 'goldendict favorites folder'
