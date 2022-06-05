


# Features
- Handles disputes
- Handles resolutions
- Handles charge backs
- Detects when a transaction is charged back


# How to use
``` bash
# Run with errors
cargo run -- test-files/example_input.csv
# Run without errors
cargo run -- test-files/example_input.csv 2> /dev/null
```

# Testing and test data

## Integration tests with csv

Running the following command will run all the integration tests. Please see the bottom of `main.rs` for a list of all the tests.
```bash
cargo test
```

# Error handling + error states
This engine performs a best effort and there are a lot of cases where things can fail. I outputted anytime there was a bug to standard error, however there are some errors that get past back to main. 

To handle these errors, I created a custom error type that wraps all the other possible errors one might encounter when using this engine.

I also made sure to pass every error up, and managed to not use a single `unwrap`. 

# Efficiency notes

## Streams
This implementation using a Reader stream for the CSV, so the entire thing is not being stored in memory at once.

## Database
I used a Database mock to manage the Client and Transaction data. I did it this way to mock what a real database would ideally look like.

## Runtime Memory efficiency
I was extremely careful when constructing and laying out how transactions were being loaded into our objects. I was able to write this entire solution without using a single clone. And reducing the number of memcpy calls can have an extreme benefit as our ingest data size scales.

## Database efficiency
Using the spec, I ensured that each piece was at the smallest atomic unit possible when being stored into our database.

## How this can be expanded using concurrency
We may be able to shard multiple threads to work on different groups of clients, since the client's transactions are independent from one another. 


Note: I don't think we would be able to concurrent view transactions clients across multiple threads in an guaranteed efficient way, because each transactions depends on the last. I think if we had a better understand of the distribution of data on the dataset we can accurately find a good transaction concurrency model.

# Docs

Run the follow command to generate docs
```
cargo doc
```
Then open the docs here: `target/doc/payments_engine/index.html`


# Questions and interpretations
There are some questions I had regarding the spec that i have outlined below, along with what i decided with.

1. If a dispute can go to a charge back and the client's funds decrease, it seems
like the only types of transactions one can dispute are deposits?
It seems like there is a 4 way matrix of disputes

- | Withdrawal | Deposit
-|-|-
Failed | Withdrawal Failed | Deposit Failed
Incorrect | Withdrawal Incorrect  | Deposit Incorrect

Failed: When a transaction did not go through.
Incorrect: When a transaction is wrong.

Chargebacks handled...
Withdrawal Failed: Chargeback will effectively resubmit a withdrawal.
Deposit Incorrect: Chargeback will effectively revert the transaction


2. This implementation assumes `locked` means you cannot make any more transactions
