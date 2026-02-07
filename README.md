## Hk 构建工具
### 目录结构：
基础mod目录</br>
mod</br>
   ├── bin</br>
   ├── include</br>
   │   ├── include_dir1</br>
   │   ├── include_dir2</br>
   │   └── ...</br>
   └── src</br>
多个mod组合成一个project
project</br>
   ├── mod1</br>
   ├── mod2</br>
   └── mod3</br>

### 指令：
#### new
创建一个空白的 hk mod

cmd:hk new projetc_name

#### 需要注意，下面几条指令在mod根目录运行会在mod层面起作用，在project跟目录运行会在project层面起作用

#### build
构建hk projetc，
自动识别mod依赖顺序，自动构建
cmd: hk build
#### run
构建hk projetc，
自动识别模块依赖顺序，自动构建并运行

cmd: hk run
#### clean
清理编译build目录
cmd: hk clean

这是一个C/C++的项目工具，基于yaml对项目进行构建，取代了对Cmake/MakeFile的编写。只要以特定的模式构建项目即可使用此工具进行构建，当前只支持linux平台</br>
ToDo：win，macos跨平台构建</br>
Todo：更灵活更强大的配置</br>
Todo：嵌入式编译支持</br>
