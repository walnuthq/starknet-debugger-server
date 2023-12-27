ALTER TABLE
    simulations
ADD
    COLUMN error_message VARCHAR,
ALTER COLUMN
    error_message
SET
    DEFAULT NULL;