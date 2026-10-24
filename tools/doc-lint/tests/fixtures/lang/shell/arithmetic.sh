#!/usr/bin/env bash
# case: << inside (( )) before a number is a shift, so the three lines below are one block
x=1
(( x << 2 ))
# one
# two
# three
echo "$x"

# case: << inside $(( )) before a name is a shift, not a heredoc swallowing the file
name=3
y=$(( x << name ))
# four
# five
# six
echo "$y"

# case: << inside a bare (( )) before a name is a shift, not a heredoc swallowing the file
(( x << name ))
# seven
# eight
# nine
echo "$x"
