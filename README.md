# Running comal in Python

This project uses pyo3 to interface Rust with Python. For more detailed information see the [pyo3 documentation]()

Create a virtual environment (using any virtualenv of your choosing) and install maturin. We will show an example with venv

```
python -m venv <venvpath>
source <venvpath>/bin/activate
pip install maturin
```

Then to build the Rust library with maturin run and run comal in python
```
maturin develop
python
> import comal
```
 
