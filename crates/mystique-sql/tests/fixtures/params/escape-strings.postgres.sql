SELECT E'it\'s :not_a_param', e'\\', :real FROM t
WHERE x = E'a\'' AND y = :y AND z = E'\\' || ':nope';
