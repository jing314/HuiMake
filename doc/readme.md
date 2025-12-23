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

#### 想要注意，下面几条指令在mod根目录运行会在mod层面起作用，在project跟目录运行会在project层面起作用

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


