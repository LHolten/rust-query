# 0.2.0

- Rewrote almost the whole library to specify the schema using enum syntax with a proc macro.
- Added a single Column type to handle a lot of query building.
- Dummy trait to retrieve multiple values at once and allow post processing.
- Added support for transactions and multiple schemas.

# 0.1.x

- This version was SQL schema first. It would generate the API based on the schema read from the database.