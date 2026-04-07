-- Bootstrap singleton income config to match legacy Laravel defaults.
INSERT INTO incomes (
    ggr,
    fee_transaction,
    fee_withdrawal,
    amount
)
SELECT
    10,
    2,
    14,
    0
WHERE NOT EXISTS (
    SELECT 1
    FROM incomes
);
