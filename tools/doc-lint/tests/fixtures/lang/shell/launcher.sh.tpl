#!/usr/bin/env bash
# case: a .sh.tpl template is lexed as shell, so a trailing # comment counts

echo "{{VALUE}}" # seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd
# one
# two
# three
