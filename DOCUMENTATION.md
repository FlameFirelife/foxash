
# Foxash

Foxash is a small programming language built around readable syntax and simple commands.

Foxash programs use the `.foxash`file extension.

Created by **FlameDev Studios**.

---

## Running Foxash

Run a program with:

```
foxash run <file.foxash>
```

Example:

```
foxash run hello.foxash
```

Check a program without running it:

```
foxash check hello.foxash
```

Other commands:

```
foxash help
foxash version
```

---

# Your first Foxash program

Create a file named `hello.foxash`:

```
write("Hello, Foxash!")
```

Run it:

```
foxash run hello.foxash
```

Output:

```text
Hello, Foxash!
```

---

# Comments

Comments begin with `//`:

```
// This line is ignored
write("This line runs")
```

Comments can also come after code:

```
write("Hello") // This is still a comment
```

---

# Variables

Use `define`to create a variable:

```
define name = "Ash"
define health = 100
define alive = true
```

Variables can be changed after they are created:

```
define health = 100

health = health - 10

write(health)
```

Output:

```text
90
```

A variable must already exist before it can be reassigned.

---

# Values

Foxash supports:

- Text
- Numbers
- Booleans
- Lists
- `nothing`

## Text

Text is written between double quotes:

```
define message = "Hello, Foxash!"
write(message)
```

## Numbers

Foxash supports whole numbers and decimal numbers:

```
define health = 100
define multiplier = 1.5
```

## Booleans

Boolean values are:

```
true
false
```

Example:

```
define game_over = false
define has_key = true
```

## Nothing

`nothing`represents an empty value:

```
define result = nothing
write(result)
```

Output:

```text
nothing
```

---

# Writing output

Use `write(...)`to print a value:

```
write("Hello!")
write(42)
write(true)
```

Variables and expressions can be written too:

```
define name = "Flame"
write("Hello, " + name + "!")
```

Output:

```text
Hello, Flame!
```

---

## Saving Lists
Lists can be saved as JSON files by using:
```foxash
save list_name to "file_name.json"
```
 For now, this isn't really usefull due to the inability to read them.

---

# Input

Use `get`to receive input from the user.

## Text input

```
get name as text = "What is your name? "

write("Hello, " + name + "!")
```

## Number input

```
get age as number = "How old are you? "

write("You are " + age + " years old.")
```

## Boolean input

```
get ready as boolean = "Are you ready? "
```

Boolean input accepts:

```text
true
false
```

If the input does not match the selected type, Foxash reports an error and stops the program.

---

# Operators

## Arithmetic

```
define addition = 10 + 5
define subtraction = 10 - 5
define multiplication = 10 * 5
define division = 10 / 5
define remainder = 10 % 3
```

| Operator | Meaning |
|---|---|
| `+`| Addition |
| `-`| Subtraction |
| `*`| Multiplication |
| `/`| Division |
| `%`| Remainder |

For example:

```
write(10 % 3)
```

Output:

```text
1
```

## Comparisons

Comparisons produce `true`or `false`:

```
score == 10
score != 10
score > 10
score < 10
score >= 10
score <= 10
```

| Operator | Meaning |
|---|---|
| `==`| Equal to |
| `!=`| Not equal to |
| `>`| Greater than |
| `<`| Less than |
| `>=`| Greater than or equal to |
| `<=`| Less than or equal to |

`=`is used for assignment:

```
score = 10
```

`==`is used for comparison:

```
when score == 10 {
    write("Perfect score.")
}
```

## Logical operators

Foxash uses readable words for logical operations:

```
when alive == true and health > 0 {
    write("The player is alive.")
}
```

```
when game_over == true or health <= 0 {
    write("The game has ended.")
}
```

```
when not finished {
    write("The work is not finished.")
}
```

| Operator | Meaning |
|---|---|
| `and`| Both conditions must be true |
| `or`| At least one condition must be true |
| `not`| Reverses a boolean value |

---

# Conditions

Use `when`to run code when a condition is true:

```
define health = 25

when health > 0 {
    write("You are still alive.")
}
```

A short one-line condition can use a colon:

```
when health <= 0: write("Game over.")
```

Conditions can contain arithmetic, comparisons, and logical operators:

```
when health > 0 and game_over == false {
    write("The game continues.")
}
```

---

# Otherwise

Use `otherwise`for the other result of a condition:

```
define health = 0

when health > 0 {
    write("You are alive.")
} otherwise {
    write("You are defeated.")
}
```

Only one branch runs:

- The `when`body runs if the condition is `true`.
- The `otherwise`body runs if the condition is `false`.

`otherwise`belongs directly after the closing `}`of the `when`block.

---

# Functions

Functions group instructions under a name.

```
function greet(name) {
    write("Hello, " + name + "!")
}
```

Call the function like this:

```
greet("Flame")
```

Output:

```text
Hello, Flame!
```

## Parameters

Parameters are the names inside the function parentheses:

```
function introduce(name, role) {
    write(name + " is a " + role + ".")
}

introduce("Ash", "wanderer")
```

Arguments are passed in the same order as the parameters.

## Giving back a value

Use `give`to return a value from a function:

```
function add(first, second) {
    give first + second
}
```

Store the result:

```
define total = add(4, 6)

write(total)
```

Output:

```text
10
```

A function can give back text, numbers, booleans, lists, or `nothing`:

```
function welcome(name) {
    give "Welcome, " + name + "!"
}

define message = welcome("Flame")
write(message)
```

`give`immediately ends the current function and sends the value back to the code that called it.

`give`can only be used inside a function.

## Functions inside functions

Functions can call other functions:

```
function double(value) {
    give value * 2
}

function quadruple(value) {
    give double(double(value))
}

write(quadruple(3))
```

Output:

```text
12
```

---

# Lists

Create a list with square brackets:

```
define inventory = ["sword", "potion", "map"]
```

Lists can contain different supported values:

```
define player = ["Ash", 100, true]
```

Print a list:

```
write(inventory)
```

Example output:

```text
[sword, potion, map]
```

## One-based indexing

Foxash lists are **one-indexed**.

That means the first item is at index `1`:

```
define names = ["Ash", "Ember", "Flame"]

write(names[1])
```

Output:

```text
Ash
```

The indexes are:

```text
names[1]  -> Ash
names[2]  -> Ember
names[3]  -> Flame
```

Index `0`is not the first item in Foxash.

Indexes must be positive whole numbers.

---

# Adding to lists

Use `add ... to ...`to add an item to the end of a list:

```
define storage = ["sword"]

add "shield" to storage
add "potion" to storage

write(storage)
```

Output:

```text
[sword, shield, potion]
```

The value being added can be an expression:

```
define item_name = "torch"
define inventory = []

add item_name to inventory
add "map" to inventory

write(inventory)
```

The list must already exist and must contain a list value.

---

# Named loops

Foxash loops have names and use square brackets.

```
loop question[
    write("This is a loop.")

    end question
]
```

`question`is only the name of this particular loop. It is not a special word.

This is also valid:

```
loop adventure[
    write("The adventure continues.")

    end adventure
]
```

## Ending a loop

Use `end`followed by the loop's name:

```
loop game[
    write("The game is finished.")

    end game
]
```

The name must match:

```
loop game[
    end game
]
```

Not:

```
loop game[
    end question
]
```

## Restarting a loop

Use `restart`followed by the loop's name:

```
define attempts = 0

loop question[
    attempts = attempts + 1
    write(attempts)

    when attempts < 3 {
        restart question
    }

    end question
]
```

`restart question`starts the loop named `question`from the beginning again.

## Input loop example

```
loop answer_loop[
    get answer as text = "Type yes: "

    when answer == "yes" {
        write("Correct.")
        end answer_loop
    } otherwise {
        write("Try again.")
        restart answer_loop
    }
]
```

The opening `[`belongs to the loop declaration, and the closing `]`finishes the loop.

---

# Waiting

Use `wait(...)`to pause the program for a number of seconds:

```
write("Starting...")
wait(2)
write("Finished.")
```

Decimal values are allowed:

```
wait(0.5)
```

The value must be zero or greater.

---

# Random numbers

Use `random(min, max)`to generate a random whole number.

The limits are inclusive:

```
define roll = random(1, 6)

write(roll)
```

The result can be any whole number from `1`to `6`.

Example:

```
define damage = random(4, 9)
write("You deal " + damage + " damage.")
```

`random(...)`requires exactly two whole-number arguments:

```
random(1, 10)
```

Invalid examples:

```
random(10, 1)
random("one", 6)
random(1.5, 6)
```

---

# Strings

Strings use double quotes:

```
define message = "Hello, Foxash!"
write(message)
```

Foxash supports these escape sequences:

```
write("Line one\nLine two")
write("A tab:\tvalue")
write("A quote: \"hello\"")
write("A backslash: \\")
```

Strings cannot continue across an unescaped newline.

---

# A complete example

```
define player_name = "Ash"
define health = 20
define gold = 5
define inventory = ["sword"]

function heal(amount) {
    health = health + amount
    give health
}

function show_status() {
    write("Player: " + player_name)
    write("Health: " + health)
    write("Gold: " + gold)
    write("Inventory: " + inventory)
}

add "potion" to inventory

define roll = random(1, 6)
write("Your roll: " + roll)

when roll >= 4 {
    write("Good roll.")
} otherwise {
    write("Bad roll.")
}

show_status()

loop adventure[
    get choice as text = "Choose: heal, status, or quit: "

    when choice == "heal" {
        when gold >= 2 {
            gold = gold - 2
            heal(10)
            write("You used 2 gold to heal.")
        } otherwise {
            write("You do not have enough gold.")
        }
    }

    when choice == "status" {
        show_status()
    }

    when choice == "quit" {
        write("The adventure ends.")
        end adventure
    } otherwise {
        when choice != "heal" and choice != "status" and choice != "quit" {
            write("That is not a valid choice.")
        }
    }
]

write("Thanks for playing.")
```

---

# CLI reference

Run a program:

```
foxash run game.foxash
```

Check a program without running it:

```
foxash check game.foxash
```

Show help:

```
foxash help
```

Show the Foxash version:

```
foxash version
```

---

# Errors

Foxash can report errors while reading, parsing, or running a program.

Common errors include:

- Missing files
- Invalid characters
- Unfinished strings
- Missing brackets or braces
- Missing values
- Undefined variables
- Invalid input types
- Invalid list indexes
- Invalid function arguments
- Division by zero
- Invalid `random(...)`arguments
- Adding to a variable that is not a list

Errors include the file location when it is available.

---

# Version

This documentation describes **Foxash v1.2.0**.

Created by **FlameDev Studios**.
