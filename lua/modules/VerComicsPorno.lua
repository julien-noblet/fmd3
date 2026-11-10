function Init()
	local m = NewWebsiteModule()
	m.ID                         = '951d32dd4fc4468692d3a3b7572a707c'
	m.Name                       = 'VerComicsPorno'
	m.RootURL                    = 'https://vercomicsporno.com'
	m.Category                   = 'Spanish'
	m.OnGetDirectoryPageNumber   = 'GetDirectoryPageNumber'
	m.OnGetNameAndLink           = 'GetNameAndLink'
	m.OnGetInfo                  = 'GetInfo'
	m.OnGetPageNumber            = 'GetPageNumber'
end

function GetDirectoryPageNumber()
	if HTTP.GET(MODULE.RootURL) then
		PAGENUMBER = tonumber(CreateTXQuery(HTTP.Document).XPathString('//div[@class="wp-pagenavi"]/a[@class="last"]/@href'):match('page/(%d+)')) or 1
		return no_error
	else
		return net_problem
	end
end

function GetNameAndLink()
	if HTTP.GET(MODULE.RootURL .. '/page/' .. (URL + 1)) then
		CreateTXQuery(HTTP.Document).XPathHREFTitleAll('//div[@class="blog-list-items"]//h2/a', LINKS, NAMES)
		return no_error
	else
		return net_problem
	end
end

function GetInfo()
	MANGAINFO.URL = MaybeFillHost(MODULE.RootURL, URL)
	if HTTP.GET(MANGAINFO.URL) then
		local x = CreateTXQuery(HTTP.Document)
		MANGAINFO.Title      = x.XPathString('//h1[@class="titl"]')
		MANGAINFO.CoverLink  = x.XPathString('//div[@class="posts"]/div[@class="wp-content"]/p/img/@src[1]')
		MANGAINFO.Genres     = x.XPathStringAll('//div[@class="tax_post"]/div[(./div="Etiquetas")]/div[@class="links"]//a')

		MANGAINFO.ChapterLinks.Add(URL)
		MANGAINFO.ChapterNames.Add(MANGAINFO.Title)
		return no_error
	else
		return net_problem
	end
end

function GetPageNumber()
	if HTTP.GET(MaybeFillHost(MODULE.RootURL, URL)) then
		CreateTXQuery(HTTP.Document).XPathStringAll('//div[@class="posts"]/div[@class="wp-content"]/p/img/@src', TASK.PageLinks)
		return true
	else
		return false
	end
end
