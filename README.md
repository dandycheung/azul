# Azul - Desktop GUI framework

## WARNING: The features advertised in this README may not work yet.

<!-- [START badges] -->
[![Build Status Linux / macOS](https://travis-ci.org/maps4print/azul.svg?branch=master)](https://travis-ci.org/maps4print/azul)
[![Build status Windows](https://ci.appveyor.com/api/projects/status/p487hewqh6bxeucv?svg=true)](https://ci.appveyor.com/project/fschutt/azul)
[![Coverage Status](https://coveralls.io/repos/github/maps4print/azul/badge.svg?branch=master)](https://coveralls.io/github/maps4print/azul?branch=master)
[![LICENSE](https://img.shields.io/badge/license-LGPL%203.0%20+%20static%20linking-blue.svg)](LICENSE)
[![Rust Compiler Version](https://img.shields.io/badge/rustc-1.45%20stable-blue.svg)]()
<!-- [END badges] -->

> Azul is a free, functional, reactive GUI framework for Rust and C++,
built using the WebRender rendering engine and a CSS / HTML-like document
object model for rapid development of beautiful, native desktop applications

###### [Website](https://azul.rs/) | [User guide](https://azul.rs/doc/) | [API documentation](https://azul.rs/api/) | [Video demo](https://www.youtube.com/watch?v=kWL0ehf4wwI) | [Matrix Chat](https://matrix.to/#/#azul:matrix.org)

## About

Azul is a library for creating graphical user interfaces in Rust and C. It mixes
paradigms from functional, reactive and data-oriented programming with an API
suitable for developing cross-platform desktop applications. The two core principles
of Azul is to not render objects that aren't visible and to use composition of DOM
trees over inheritance.

Azul separates the concerns of business logic / callbacks, data model and UI
rendering / styling by not letting the UI / rendering logic have mutable access
to the application data. In Azul, rendering the view is a pure function that maps
your application data to a styled DOM. "Widgets" are just functions that render
a certain state, more complex widgets use function composition.

Since recreating DOM objects is expensive (note: "expensive" = 3 milliseconds),
Azul caches the DOM object and does NOT recreate it on every frame - only
when callbacks request to recreate it.

The application and widget data is managed using a reference-counted
boxed type (`RefAny`), which can be downcasted to a concrete type if
necessary. Widget-local data that needs to be retained between frames is
stored on the DOM nodes themselves, similar to how the HTML `dataset`
property can be used to store widget data.

## Installation

Due to its relatively large size (and to provide C / C++ interop),
azul is built as a dynamic library in the `azul-dll` package. You can
download pre-built binaries from [azul.rs/releases](https://azul.rs/releases).

### Prerequisites / system dependencies

#### Linux

On Linux, you need to install the following packages:

```sh
libfreetype6-dev # needed to render fonts
```

**Arch Linux**: The package for `libfreetype6-dev` is called `freetype`.

If you publish an azul-based GUI application, you need to remember to
include these dependencies in your package description, otherwise your
users won't be able to start the application.

#### Windows / Mac

You do not need to install anything, azul uses the standard system APIs to
render / select fonts.

### Installation using pre-built-binaries

1. Download the library from [azul.rs/releases](https://azul.rs/releases)
2. Set your linker to link against the library
    - Rust: Set `AZUL_LINK_PATH` environment variable to the path of the library
    - C / C++: Copy the `azul.h` on the release page to your project headers
        and the `azul.dll` to your IDE project.

The API for Rust, C++ and other languages is exactly the same,
since the API is auto-generated by the `build.py` script.
If you want to generate language bindings for your language,
you can generate them using the `public.api.json` file.

*To run programs on Linux, you may also need to copy the
`libazul.so` into `/usr/lib`.* Eventually this will be solved
by upstreaming the library into repositories once all major
bugs are resolved.

#### Building from source (crates.io)

By default, you should be able to run

```sh
cargo install --version 1.0.0 azul-dll
```

to compile the DLL from crates.io. The library will be built
and installed in the `$AZUL_LINK_PATH` directory, which defaults to
`$CARGO_HOME_DIR/lib/azul-dll-0.1.0/target/release/`

### Building from source (git)

Building the library from source requires clang as well as
the prerequisites listed above.

```sh
git clone https://github.com/maps4print/azul
cd azul-dll
cargo build --release
```

This command should produce an `azul.dll` file in the
`/target/release` folder, in order to use this, you will
also need to set `AZUL_LINK_PATH` to `$BUILD_DIR/target/release/`.

If you are developing on the library, you may also need to
re-generate the Rust / C API, in which case you should prefer
to use the `build.py` script:

```
python3 ./build.py
```

## Example

Note: The widgets are custom to each programming language. All callbacks
have to use `extern "C"` in order to be compatible with the library.
The binary layout of all API types is described in the[`api.json` file](./api.json).

[See the /examples folder for example code in different languages]
(https://github.com/maps4print/azul/tree/master/examples)

![Hello World Application](https://i.imgur.com/KkqB2E5.png)

### Rust

```rust
use azul::prelude::*;
use azul_widgets::{button::Button, label::Label};

struct DataModel {
    counter: usize,
}

// Model -> View
extern "C" fn render_my_view(data: &RefAny, _: LayoutInfo) -> StyledDom {

    let mut result = StyledDom::default();

    let data = match data.downcast_ref::<DataModel>() {
        Some(s) => s,
        None => return result,
    };

    let label = Label::new(format!("{}", data.counter)).dom();
    let button = Button::with_label("Update counter")
        .onmouseup(update_counter, data.clone())
        .dom();

    result
    .append(label)
    .append(button)
}

// View updates model
extern "C" fn update_counter(data: &mut RefAny, event: CallbackInfo) -> UpdateScreen {
    let mut data = match data.downcast_mut::<DataModel>() {
        Some(s) => s,
        None => return UpdateScreen::DoNothing,
    };
    data.counter += 1;
    UpdateScreen::RegenerateDomForCurrentWindow
}

fn main() {
    let app = App::new(RefAny::new(DataModel { counter: 0 }), AppConfig::default());
    app.run(WindowCreateOptions::new(render_my_view));
}
```

### C++

```cpp
#include "azul.h"
#include "azul-widgets.h"

using namespace azul;
using namespace azul.widgets.button;
using namespace azul.widgets.label;

struct DataModel {
    counter: uint32_t
}

// Model -> View
StyledDom render_my_view(const RefAny& data, LayoutInfo info) {

    auto result = StyledDom::default();

    const DataModel* data = data.downcast_ref();
    if !(data) {
        return result;
    }

    auto label = Label::new(String::format("{counter}", &[data.counter])).dom();
    auto button = Button::with_label("Update counter")
       .onmouseup(update_counter, data.clone())
       .dom();

    result = result
        .append(label)
        .append(button);

    return result;
}

UpdateScreen update_counter(RefAny& data, CallbackInfo event) {
    DataModel data = data.downcast_mut().unwrap();
    data.counter += 1;
    return UpdateScreen::RegenerateDomForCurrentWindow;
}

int main() {
    auto app = App::new(RefAny::new(DataModel { .counter = 0 }), AppConfig::default());
    app.run(WindowCreateOptions::new(render_my_view));
}
```

### C

```c
#include "azul.h"

typedef struct {
    uint32_t counter;
} DataModel;

void DataModel_delete(DataModel* restrict A) { }
AZ_REFLECT(DataModel, DataModel_delete);

AzStyledDom render_my_view(AzRefAny* restrict data, AzLayoutInfo info) {

    AzString counter_string;

    DataModelRef d = DataModelRef_create(data);
    if (DataModel_downcastRef(data, &d)) {
        AzFmtArgVec fmt_args = AzFmtArgVec_fromConstArray({{
            .key = AzString_fromConstStr("counter"),
            .value = AzFmtValue_Uint(d.ptr->counter)
        }});
        counter_string = AzString_format(AzString_fromConstStr("{counter}"), fmt_args);
    } else {
        return AzStyledDom_empty();
    }
    DataModelRef_delete(&d);

    AzDom const html = {
        .root = AzNodeData_new(AzNodeType_Body),
        .children = AzDomVec_fromConstArray({AzDom_new(AzNodeType_Label(counter_string))}),
        .total_children = 1, // len(children)
    };
    AzCss const css = AzCss_fromString(AzString_fromConstStr("body { font-size: 50px; }"));
    return AzStyledDom_new(html, css);
}

UpdateScreen update_counter(RefAny& data, CallbackInfo event) {
    DataModelRefMut d = DataModelRefMut_create(data);
    if !(DataModel_downcastRef(data, &d)) {
        return UpdateScreen_DoNothing;
    }
    d->ptr.counter += 1;
    DataModelRefMut_delete(&d);
    return UpdateScreen_RegenerateDomForCurrentWindow;
}

int main() {
    DataModel model = { .counter = 5 };
    AzApp app = AzApp_new(DataModel_upcast(model), AzAppConfig_default());
    AzApp_run(app, AzWindowCreateOptions_new(render_my_view));
    return 0;
}
```

## Documentation

The documentation is built using the `build.py` script, which
will generate the entire `azul.rs` website in the `/target/html`
directory:

```
python3 ./build.py
```

- Class documentation is available at [azul.rs/api](https://azul.rs/api/)
- Tutorials / examples / user guide is available under [azul.rs/doc](https://azul.rs/doc/).

NOTE: The class documentation can also be printed as a
PDF if you prefer that.

## License

This library is licensed under the LGPL version 3.0 with an
exception for static linking. Similar to the FLTK and wxWidgets
license,

Copyright 2017 - current Felix Schütt
