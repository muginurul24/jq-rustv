DELETE FROM incomes
WHERE id IN (
    SELECT id
    FROM incomes
    WHERE ggr = 10
      AND fee_transaction = 2
      AND fee_withdrawal = 14
      AND amount = 0
    ORDER BY id
    LIMIT 1
);
