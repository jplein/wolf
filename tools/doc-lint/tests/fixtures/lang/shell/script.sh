#!/usr/bin/env bash
# case: the shebang is not a comment, so this two-line block passes
# (the second line of that block)

# case: a # that does not start a word is not a comment

n=$# ; echo seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd
len=${#n} ; echo seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd
trimmed=${n#y} ; echo seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd
word=a#b ; echo seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd
quoted="# seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd"
single='# seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd'
dollar=$'# seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd'

# case: a here-string is not a heredoc

tr a b <<< hello

# case: heredoc bodies are not comments

cat <<EOF
# body one
# body two
# body three
EOF
cat <<'EOF'
# quoted body one
# quoted body two
# quoted body three
EOF
cat <<"EOF"
# double-quoted body one
# double-quoted body two
# double-quoted body three
EOF
cat <<-EOF
	# tab body one
	# tab body two
	# tab body three
	EOF

# case: a # inside $( ) is still a comment

out=$(echo hi # seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd
)

# case: three # lines fail

# one
# two
# three
echo "$out $len $trimmed $word $quoted $single $dollar"
