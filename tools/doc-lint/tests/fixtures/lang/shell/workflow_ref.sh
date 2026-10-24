#!/usr/bin/env bash
# case: a plan citation in a comment is reported, the same text echoed is not

# per the plan
echo "per the plan"
echo per the plan

# case: a trailing comment is scanned

echo hi # OD7 says so

# case: a heredoc body is not a comment

cat <<EOF
# per the plan
EOF
