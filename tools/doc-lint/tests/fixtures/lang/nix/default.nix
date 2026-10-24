# case: # lines inside '' strings, around ''' ''$ and ''\ escapes, are not comments

{
  script = ''
    # inside one
    # inside two
    # inside three
    echo '''quoted
    # after triple one
    # after triple two
    # after triple three
    echo ''${notInterpolated}
    # after dollar one
    # after dollar two
    # after dollar three
    echo ''\t ${toString 1}
    # after backslash one
    # after backslash two
    # after backslash three
  '';

  # case: an antiquote holding a string with # is not a comment

  s = "${ "#" } # seg01abcd seg02abcd seg03abcd seg04abcd seg05abcd seg06abcd seg07abcd seg08abcd seg09abcd seg10abcd";

  # case: three # lines fail

  # one
  # two
  # three
  a = 1;

  /* case: a three-line block comment fails
     second
     third */
  b = 2;

  /* a two-line block
     passes */
  c = 3;
}
