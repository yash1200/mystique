SELECT ':x', "c:y", $$ :z $$, :a -- :b
/* :c */ FROM t WHERE id = :a AND v = :b2;
